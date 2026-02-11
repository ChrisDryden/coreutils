// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore (ToDO) accum bcost breakwords lcost linebreak linebreaking linebreaks linelen maxlength minlength nchars ostream overlen parasplit plass posn punct slen sstart tabwidth tlen underlen wcost winfo wlen wordlen

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
    fn compute_width(&self, winfo: &WordInfo, posn: usize, fresh: bool) -> usize {
        if fresh {
            0
        } else {
            let post = winfo.after_tab;
            match winfo.before_tab {
                None => post,
                Some(pre) => {
                    post + ((pre + posn) / self.opts.tabwidth + 1) * self.opts.tabwidth - posn
                }
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
fn break_simple<'a, T: Iterator<Item = &'a WordInfo<'a>>>(
    mut iter: T,
    args: &mut BreakArgs<'a>,
) -> std::io::Result<()> {
    iter.try_fold((args.init_len, false), |(l, prev_punct), winfo| {
        accum_words_simple(args, l, prev_punct, winfo)
    })?;
    args.ostream.write_all(b"\n")
}

fn accum_words_simple<'a>(
    args: &mut BreakArgs<'a>,
    l: usize,
    prev_punct: bool,
    winfo: &'a WordInfo<'a>,
) -> std::io::Result<(usize, bool)> {
    // compute the length of this word, considering how tabs will expand at this position on the line
    let wlen = winfo.word_nchars + args.compute_width(winfo, l, false);

    let slen = compute_slen(
        args.uniform,
        winfo.new_line,
        winfo.sentence_start,
        prev_punct,
    );

    if l + wlen + slen > args.opts.width {
        write_newline(args.indent, args.ostream)?;
        write_with_spaces(&winfo.word[winfo.word_start..], 0, args.ostream)?;
        Ok((args.indent_len + winfo.word_nchars, winfo.ends_punct))
    } else {
        write_with_spaces(winfo.word, slen, args.ostream)?;
        Ok((l + wlen + slen, winfo.ends_punct))
    }
}

/// `break_knuth_plass` implements an "optimal" breaking algorithm in the style of
/// Knuth, D.E., and Plass, M.F. "Breaking Paragraphs into Lines." in Software,
/// Practice and Experience. Vol. 11, No. 11, November 1981.
/// <http://onlinelibrary.wiley.com/doi/10.1002/spe.4380111102/pdf>
fn break_knuth_plass<'a, T: Clone + Iterator<Item = &'a WordInfo<'a>>>(
    mut iter: T,
    args: &mut BreakArgs<'a>,
) -> std::io::Result<()> {
    // run the algorithm to get the breakpoints
    let breakpoints = find_kp_breakpoints(iter.clone(), args);

    // iterate through the breakpoints (note that breakpoints is in reverse break order, so we .rev() it
    let result: std::io::Result<(bool, bool)> = breakpoints.iter().rev().try_fold(
        (false, false),
        |(mut prev_punct, mut fresh), &(next_break, break_before)| {
            if fresh {
                write_newline(args.indent, args.ostream)?;
            }
            // at each breakpoint, keep emitting words until we find the word matching this breakpoint
            for winfo in &mut iter {
                let (slen, word) = slice_if_fresh(
                    fresh,
                    winfo.word,
                    winfo.word_start,
                    args.uniform,
                    winfo.new_line,
                    winfo.sentence_start,
                    prev_punct,
                );
                fresh = false;
                prev_punct = winfo.ends_punct;

                // We find identical breakpoints here by comparing addresses of the references.
                // This is OK because the backing vector is not mutating once we are linebreaking.
                if std::ptr::eq(winfo, next_break) {
                    // OK, we found the matching word
                    if break_before {
                        write_newline(args.indent, args.ostream)?;
                        write_with_spaces(&winfo.word[winfo.word_start..], 0, args.ostream)?;
                    } else {
                        // breaking after this word, so that means "fresh" is true for the next iteration
                        write_with_spaces(word, slen, args.ostream)?;
                        fresh = true;
                    }
                    break;
                }
                write_with_spaces(word, slen, args.ostream)?;
            }
            Ok((prev_punct, fresh))
        },
    );
    let (mut prev_punct, mut fresh) = result?;

    // after the last linebreak, write out the rest of the final line.
    for winfo in iter {
        if fresh {
            write_newline(args.indent, args.ostream)?;
        }
        let (slen, word) = slice_if_fresh(
            fresh,
            winfo.word,
            winfo.word_start,
            args.uniform,
            winfo.new_line,
            winfo.sentence_start,
            prev_punct,
        );
        prev_punct = winfo.ends_punct;
        fresh = false;
        write_with_spaces(word, slen, args.ostream)?;
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
        let wlen = words[j].word_nchars + args.compute_width(words[j], len, false);
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
fn find_kp_breakpoints<'a, T: Iterator<Item = &'a WordInfo<'a>>>(
    iter: T,
    args: &BreakArgs<'a>,
) -> Vec<(&'a WordInfo<'a>, bool)> {
    let words: Vec<&WordInfo> = iter.collect();
    let n = words.len();
    if n == 0 {
        return vec![];
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
        breaks.push((words[idx], true));
        idx = next_brk[idx];
    }
    breaks.reverse();
    breaks
}

/// Number of spaces to add before a word, based on mode, newline, sentence start.
fn compute_slen(uniform: bool, newline: bool, start: bool, punct: bool) -> usize {
    if uniform || newline {
        if start || (newline && punct) { 2 } else { 1 }
    } else {
        0
    }
}

/// If we're on a fresh line, `slen=0` and we slice off leading whitespace.
/// Otherwise, compute `slen` and leave whitespace alone.
fn slice_if_fresh(
    fresh: bool,
    word: &[u8],
    start: usize,
    uniform: bool,
    newline: bool,
    sstart: bool,
    punct: bool,
) -> (usize, &[u8]) {
    if fresh {
        (0, &word[start..])
    } else {
        (compute_slen(uniform, newline, sstart, punct), word)
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
