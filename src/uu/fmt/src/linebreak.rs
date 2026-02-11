// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore (ToDO) bcost lcost linebreak maxlength nchars ostream parasplit plass posn punct slen tabwidth wcost winfo wlen

use std::io::{BufWriter, Stdout, Write};

use crate::FmtOptions;
use crate::parasplit::{ParaWords, Paragraph, WordInfo};

struct BreakArgs<'a> {
    opts: &'a FmtOptions,
    init_len: usize,
    indent: &'a [u8],
    indent_len: usize,
    uniform: bool,
    ostream: &'a mut BufWriter<Stdout>,
}

impl BreakArgs<'_> {
    fn compute_width(&self, winfo: &WordInfo, posn: usize) -> usize {
        let post = winfo.after_tab;
        match winfo.before_tab {
            None => post,
            Some(pre) => {
                post + ((pre + posn) / self.opts.tabwidth + 1) * self.opts.tabwidth - posn
            }
        }
    }
}

pub fn break_lines(
    para: &Paragraph,
    opts: &FmtOptions,
    ostream: &mut BufWriter<Stdout>,
) -> std::io::Result<()> {
    // indent
    let p_indent = &para.indent_str;
    let p_indent_len = para.indent_len;

    // words
    let p_words = ParaWords::new(opts, para);
    let mut p_words_words = p_words.words();

    // the first word will *always* appear on the first line
    // make sure of this here
    let Some(winfo) = p_words_words.next() else {
        return ostream.write_all(b"\n");
    };

    // print the init, if it exists, and get its length
    let p_init_len = winfo.word_nchars
        + if opts.crown || opts.tagged {
            // handle "init" portion
            ostream.write_all(&para.init_str)?;
            para.init_len
        } else if !para.mail_header {
            // for non-(crown, tagged) that's the same as a normal indent
            ostream.write_all(p_indent)?;
            p_indent_len
        } else {
            // except that mail headers get no indent at all
            0
        };

    // write first word after writing init
    ostream.write_all(winfo.word)?;

    // does this paragraph require uniform spacing?
    let uniform = para.mail_header || opts.uniform;

    let mut break_args = BreakArgs {
        opts,
        init_len: p_init_len,
        indent: p_indent,
        indent_len: p_indent_len,
        uniform,
        ostream,
    };

    if opts.quick || para.mail_header {
        break_simple(p_words_words, &mut break_args)
    } else {
        break_knuth_plass(p_words_words, &mut break_args)
    }
}

/// `break_simple` implements a "greedy" breaking algorithm: print words until
/// maxlength would be exceeded, then print a linebreak and indent and continue.
fn break_simple<'a>(
    iter: impl Iterator<Item = &'a WordInfo<'a>>,
    args: &mut BreakArgs<'a>,
) -> std::io::Result<()> {
    let mut l = args.init_len;
    let mut prev_punct = false;
    for winfo in iter {
        let wlen = winfo.word_nchars + args.compute_width(winfo, l);
        let slen = compute_slen(args.uniform, winfo.new_line, winfo.sentence_start, prev_punct);
        if l + wlen + slen > args.opts.width {
            write_newline(args.indent, args.ostream)?;
            write_with_spaces(&winfo.word[winfo.word_start..], 0, args.ostream)?;
            l = args.indent_len + winfo.word_nchars;
        } else {
            write_with_spaces(winfo.word, slen, args.ostream)?;
            l += wlen + slen;
        }
        prev_punct = winfo.ends_punct;
    }
    args.ostream.write_all(b"\n")
}

/// `break_knuth_plass` implements an "optimal" breaking algorithm in the style of
/// Knuth, D.E., and Plass, M.F. "Breaking Paragraphs into Lines." in Software,
/// Practice and Experience. Vol. 11, No. 11, November 1981.
/// <http://onlinelibrary.wiley.com/doi/10.1002/spe.4380111102/pdf>
fn break_knuth_plass<'a>(
    iter: impl Iterator<Item = &'a WordInfo<'a>>,
    args: &mut BreakArgs<'a>,
) -> std::io::Result<()> {
    let (words, breaks) = find_kp_breakpoints(iter, args);
    let mut prev_punct = false;
    let mut brk = 0;
    for (i, w) in words.iter().enumerate() {
        if brk < breaks.len() && i == breaks[brk] {
            write_newline(args.indent, args.ostream)?;
            write_with_spaces(&w.word[w.word_start..], 0, args.ostream)?;
            brk += 1;
        } else {
            let slen = compute_slen(args.uniform, w.new_line, w.sentence_start, prev_punct);
            write_with_spaces(w.word, slen, args.ostream)?;
        }
        prev_punct = w.ends_punct;
    }
    args.ostream.write_all(b"\n")
}

/// GNU-compatible cost functions: EQUIV(n) = n*n, SHORT_COST(n) = EQUIV(n*10)
fn short_cost(d: i64) -> i64 {
    (d * 10) * (d * 10)
}

fn ragged_cost(d: i64) -> i64 {
    short_cost(d) / 2
}

const LINE_COST: i64 = 70 * 70;
const SENTENCE_BONUS: i64 = 50 * 50;
const NOBREAK_COST: i64 = 600 * 600;

/// Scan forward from `first_word`, extending a line that starts at `init_len`,
/// and return `(cost, break_index, line_length)` for the best break point.
fn best_break(
    words: &[&WordInfo],
    init_len: usize,
    first_word: usize,
    prev_punct: bool,
    args: &BreakArgs,
    best_cost: &[i64],
    next_brk: &[usize],
    line_len: &[usize],
) -> (i64, usize, usize) {
    let n = words.len();
    let goal = args.opts.goal as i64;

    let mut best = i64::MAX;
    let mut best_j = first_word;
    let mut best_ll = init_len;
    let mut len = init_len;
    let mut prev_punct = prev_punct;

    let mut j = first_word;
    loop {
        let lcost = if j >= n {
            0
        } else {
            let mut c = short_cost(goal - len as i64);
            if next_brk[j] < n {
                c += ragged_cost(len as i64 - line_len[j] as i64);
            }
            c
        };

        let wcost = lcost.saturating_add(best_cost[j]);
        if wcost < best {
            best = wcost;
            best_j = j;
            best_ll = len;
        }

        if j >= n {
            break;
        }

        let slen = compute_slen(
            args.uniform,
            words[j].new_line,
            words[j].sentence_start,
            prev_punct,
        );
        let wlen = words[j].word_nchars + args.compute_width(words[j], len);
        len += slen + wlen;
        prev_punct = words[j].ends_punct;
        j += 1;

        if len > args.opts.width {
            break;
        }
    }

    (best, best_j, best_ll)
}

/// GNU-compatible backward dynamic programming for optimal line breaking.
/// Uses the same cost functions as GNU fmt to produce identical output.
fn find_kp_breakpoints<'a>(
    iter: impl Iterator<Item = &'a WordInfo<'a>>,
    args: &BreakArgs<'a>,
) -> (Vec<&'a WordInfo<'a>>, Vec<usize>) {
    let words: Vec<&WordInfo> = iter.collect();
    let n = words.len();
    if n == 0 {
        return (words, vec![]);
    }

    let is_final = |i: usize| -> bool {
        i == n - 1 || words[i + 1].sentence_start || (words[i + 1].new_line && words[i].ends_punct)
    };

    let mut best_cost = vec![0i64; n + 1];
    let mut next_brk = vec![n; n];
    let mut line_len = vec![0usize; n];

    for start in (0..n).rev() {
        let (best, brk, ll) = best_break(
            &words,
            args.indent_len + words[start].word_nchars,
            start + 1,
            words[start].ends_punct,
            args,
            &best_cost,
            &next_brk,
            &line_len,
        );
        next_brk[start] = brk;
        line_len[start] = ll;

        let mut bcost = LINE_COST;
        if start > 0 && words[start - 1].ends_punct {
            if is_final(start - 1) {
                bcost -= SENTENCE_BONUS;
            } else {
                bcost += NOBREAK_COST;
            }
        }
        if is_final(start) {
            bcost += 150_i64 * 150 / (words[start].word_nchars as i64 + 2);
        }

        best_cost[start] = best.saturating_add(bcost);
    }

    let (_, line1_break, _) = best_break(
        &words,
        args.init_len,
        0,
        false,
        args,
        &best_cost,
        &next_brk,
        &line_len,
    );

    let mut breaks = vec![];
    let mut idx = line1_break;
    while idx < n {
        breaks.push(idx);
        idx = next_brk[idx];
    }
    (words, breaks)
}

/// Number of spaces to add before a word, based on mode, newline, sentence start.
fn compute_slen(uniform: bool, newline: bool, start: bool, punct: bool) -> usize {
    if uniform || newline {
        if start || (newline && punct) { 2 } else { 1 }
    } else {
        0
    }
}

/// Write a newline and add the indent.
fn write_newline(indent: &[u8], ostream: &mut BufWriter<Stdout>) -> std::io::Result<()> {
    ostream.write_all(b"\n")?;
    ostream.write_all(indent)
}

/// Write the word, along with slen spaces.
fn write_with_spaces(
    word: &[u8],
    slen: usize,
    ostream: &mut BufWriter<Stdout>,
) -> std::io::Result<()> {
    if slen == 2 {
        ostream.write_all(b"  ")?;
    } else if slen == 1 {
        ostream.write_all(b" ")?;
    }
    ostream.write_all(word)
}
