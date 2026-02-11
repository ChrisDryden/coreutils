// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore (ToDO) bcost lcost linebreak maxlength nchars ostream parasplit posn punct slen tabwidth wcost wlen

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
    fn compute_width(&self, w: &WordInfo, posn: usize) -> usize {
        w.before_tab.map_or(w.after_tab, |pre| {
            w.after_tab + ((pre + posn) / self.opts.tabwidth + 1) * self.opts.tabwidth - posn
        })
    }

    fn write_newline(&mut self) -> std::io::Result<()> {
        self.ostream.write_all(b"\n")?;
        self.ostream.write_all(self.indent)
    }

    fn write_word(&mut self, word: &[u8], slen: usize) -> std::io::Result<()> {
        match slen {
            2 => self.ostream.write_all(b"  ")?,
            1 => self.ostream.write_all(b" ")?,
            _ => {}
        }
        self.ostream.write_all(word)
    }
}

pub fn break_lines(
    para: &Paragraph,
    opts: &FmtOptions,
    ostream: &mut BufWriter<Stdout>,
) -> std::io::Result<()> {
    let indent = &para.indent_str;
    let indent_len = para.indent_len;

    let para_words = ParaWords::new(opts, para);
    let mut iter = para_words.words();

    let Some(first) = iter.next() else {
        return ostream.write_all(b"\n");
    };

    let init_len = first.word_nchars
        + if opts.crown || opts.tagged {
            ostream.write_all(&para.init_str)?;
            para.init_len
        } else if !para.mail_header {
            ostream.write_all(indent)?;
            indent_len
        } else {
            0
        };

    ostream.write_all(first.word)?;

    let uniform = para.mail_header || opts.uniform;
    let mut args = BreakArgs {
        opts,
        init_len,
        indent,
        indent_len,
        uniform,
        ostream,
    };

    let words: Vec<&WordInfo> = iter.collect();
    let breaks = if opts.quick || para.mail_header {
        find_greedy_breakpoints(&words, &args)
    } else {
        find_optimal_breakpoints(&words, &args)
    };
    emit_words(&words, &breaks, &mut args)
}

/// Emit words, inserting line breaks at the specified indices.
fn emit_words(words: &[&WordInfo], breaks: &[usize], args: &mut BreakArgs) -> std::io::Result<()> {
    let mut breaks = breaks.iter().peekable();
    let mut prev_punct = false;
    for (i, w) in words.iter().enumerate() {
        if breaks.peek() == Some(&&i) {
            breaks.next();
            args.write_newline()?;
            args.write_word(&w.word[w.word_start..], 0)?;
        } else {
            let slen = compute_slen(args.uniform, w.new_line, w.sentence_start, prev_punct);
            args.write_word(w.word, slen)?;
        }
        prev_punct = w.ends_punct;
    }
    args.ostream.write_all(b"\n")
}

/// Greedy breaking: break before a word when it would exceed the line width.
fn find_greedy_breakpoints(words: &[&WordInfo], args: &BreakArgs) -> Vec<usize> {
    let mut breaks = vec![];
    let mut l = args.init_len;
    let mut prev_punct = false;
    for (i, w) in words.iter().enumerate() {
        let wlen = w.word_nchars + args.compute_width(w, l);
        let slen = compute_slen(args.uniform, w.new_line, w.sentence_start, prev_punct);
        if l + wlen + slen > args.opts.width {
            breaks.push(i);
            l = args.indent_len + w.word_nchars;
        } else {
            l += wlen + slen;
        }
        prev_punct = w.ends_punct;
    }
    breaks
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
            short_cost(goal - len as i64)
                + if next_brk[j] < n {
                    ragged_cost(len as i64 - line_len[j] as i64)
                } else {
                    0
                }
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

/// Backward DP for optimal line breaking. For each word position, computes
/// the minimum-cost way to set the remaining text using GNU fmt's cost model.
fn find_optimal_breakpoints(words: &[&WordInfo], args: &BreakArgs) -> Vec<usize> {
    let n = words.len();
    if n == 0 {
        return vec![];
    }

    let is_final = |i: usize| -> bool {
        i == n - 1 || words[i + 1].sentence_start || (words[i + 1].new_line && words[i].ends_punct)
    };

    let base_cost = |start: usize| -> i64 {
        let mut c = LINE_COST;
        if start > 0 && words[start - 1].ends_punct {
            if is_final(start - 1) {
                c -= SENTENCE_BONUS;
            } else {
                c += NOBREAK_COST;
            }
        }
        if is_final(start) {
            c += 150_i64 * 150 / (words[start].word_nchars as i64 + 2);
        }
        c
    };

    let mut best_cost = vec![0i64; n + 1];
    let mut next_brk = vec![n; n];
    let mut line_len = vec![0usize; n];

    for start in (0..n).rev() {
        let (best, brk, ll) = best_break(
            words,
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
        best_cost[start] = best.saturating_add(base_cost(start));
    }

    let (_, line1_break, _) = best_break(
        words,
        args.init_len,
        0,
        false,
        args,
        &best_cost,
        &next_brk,
        &line_len,
    );

    std::iter::successors(
        (line1_break < n).then_some(line1_break),
        |&idx| (next_brk[idx] < n).then_some(next_brk[idx]),
    )
    .collect()
}

/// Number of spaces to add before a word, based on mode, newline, sentence start.
fn compute_slen(uniform: bool, newline: bool, sentence_start: bool, prev_punct: bool) -> usize {
    match (
        uniform || newline,
        sentence_start || (newline && prev_punct),
    ) {
        (true, true) => 2,
        (true, false) => 1,
        _ => 0,
    }
}
