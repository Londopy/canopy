//! Counting words in LaTeX source, per section, for the writing dashboard.
//!
//! Like `texcount`'s default rules, roughly: text counts, commands and their
//! options don't, comments don't, and math counts as one "formula" rather
//! than as words. Arguments of text commands (`\emph{...}`, `\section{...}`)
//! are text; arguments of everything else (`\label{...}`, `\cite{...}`,
//! `\includegraphics{...}`) aren't. Good enough to track progress, which is
//! what a dashboard needs; it isn't a typesetter.

use serde::Serialize;

/// Words under one heading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Section {
    /// `section`, `subsection`, `chapter`, … or `preamble` for text before
    /// the first heading.
    pub level: String,
    /// The heading's text.
    pub title: String,
    /// The 1-based line it starts on.
    pub line: u32,
    /// Words in the body, not counting the heading itself.
    pub words: u32,
    /// Math expressions (inline and display).
    pub formulas: u32,
}

/// A document's counts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Count {
    /// Words in all body text.
    pub words: u32,
    /// Words in headings.
    pub heading_words: u32,
    /// Math expressions.
    pub formulas: u32,
    /// The sections, in order.
    pub sections: Vec<Section>,
}

const HEADINGS: &[&str] = &[
    "part",
    "chapter",
    "section",
    "subsection",
    "subsubsection",
    "paragraph",
];

// commands whose argument is text that counts
const TEXT_ARGS: &[&str] = &[
    "emph",
    "textbf",
    "textit",
    "textsc",
    "texttt",
    "textrm",
    "textsf",
    "underline",
    "caption",
    "footnote",
    "title",
    "item",
    "mbox",
    "text",
    "textup",
    "textsl",
    "uppercase",
    "lowercase",
];

// environments whose contents never count
const SKIP_ENVS: &[&str] = &[
    "equation",
    "equation*",
    "align",
    "align*",
    "gather",
    "gather*",
    "multline",
    "multline*",
    "displaymath",
    "math",
    "verbatim",
    "lstlisting",
    "minted",
    "tikzpicture",
    "comment",
    "eqnarray",
    "eqnarray*",
];

fn strip_comments(src: &str) -> String {
    src.lines()
        .map(|line| {
            let b = line.as_bytes();
            let mut i = 0;
            while i < b.len() {
                if b[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if b[i] == b'%' {
                    return &line[..i];
                }
                i += 1;
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn count_words(text: &str) -> u32 {
    text.split(|c: char| c.is_whitespace() || matches!(c, '~' | '/' | '—' | '–'))
        .filter(|w| w.chars().any(|c| c.is_alphanumeric()))
        .count() as u32
}

// the balanced {...} starting at `at` (which must be '{'); returns its
// contents and the index after the closing brace
fn group(s: &[char], at: usize) -> Option<(String, usize)> {
    if s.get(at) != Some(&'{') {
        return None;
    }
    let mut depth = 0;
    let mut i = at;
    while i < s.len() {
        match s[i] {
            '\\' => i += 1,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((s[at + 1..i].iter().collect(), i + 1));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn skip_space_and_options(s: &[char], mut i: usize) -> usize {
    loop {
        while i < s.len() && s[i] == ' ' {
            i += 1;
        }
        if s.get(i) == Some(&'*') {
            i += 1;
            continue;
        }
        if s.get(i) == Some(&'[') {
            let mut depth = 0;
            while i < s.len() {
                match s[i] {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            continue;
        }
        return i;
    }
}

struct Counter {
    count: Count,
    current: Section,
    // body text of the current section, counted when it ends
    text: String,
}

impl Counter {
    fn flush(&mut self) {
        self.current.words += count_words(&self.text);
        self.text.clear();
    }

    fn start(&mut self, level: &str, title: &str, line: u32) {
        self.flush();
        let done = std::mem::replace(
            &mut self.current,
            Section {
                level: level.into(),
                title: title.trim().to_string(),
                line,
                words: 0,
                formulas: 0,
            },
        );
        if done.level != "preamble" || done.words > 0 || done.formulas > 0 {
            self.count.sections.push(done);
        }
        self.count.heading_words += count_words(title);
    }
}

/// Counts a LaTeX source file. Only the document body is counted when there
/// is a `\begin{document}`; a file without one (a chapter brought in with
/// `\input`) counts whole.
pub fn count(src: &str) -> Count {
    let src = strip_comments(src);
    let body_offset = src
        .find("\\begin{document}")
        .map(|i| i + "\\begin{document}".len());
    let start_line = body_offset
        .map(|o| src[..o].matches('\n').count() as u32 + 1)
        .unwrap_or(1);
    let body = match body_offset {
        Some(o) => &src[o..],
        None => &src[..],
    };
    let body = body.split("\\end{document}").next().unwrap_or(body);
    let s: Vec<char> = body.chars().collect();

    let mut c = Counter {
        count: Count::default(),
        current: Section {
            level: "preamble".into(),
            title: String::new(),
            line: start_line,
            words: 0,
            formulas: 0,
        },
        text: String::new(),
    };
    let mut line = start_line;
    let mut i = 0;
    while i < s.len() {
        let ch = s[i];
        if ch == '\n' {
            line += 1;
            c.text.push(' ');
            i += 1;
            continue;
        }
        // inline math: $...$, $$...$$, \( \), \[ \]
        if ch == '$' {
            let double = s.get(i + 1) == Some(&'$');
            let close: &[char] = if double { &['$', '$'] } else { &['$'] };
            let mut j = i + close.len();
            while j < s.len() && !(s[j..].starts_with(close) && s[j - 1] != '\\') {
                if s[j] == '\n' {
                    line += 1;
                }
                j += 1;
            }
            c.current.formulas += 1;
            c.text.push(' ');
            i = (j + close.len()).min(s.len());
            continue;
        }
        if ch == '\\' {
            let next = s.get(i + 1).copied().unwrap_or(' ');
            if next == '(' || next == '[' {
                let close = if next == '(' { ')' } else { ']' };
                let mut j = i + 2;
                while j + 1 < s.len() && !(s[j] == '\\' && s[j + 1] == close) {
                    if s[j] == '\n' {
                        line += 1;
                    }
                    j += 1;
                }
                c.current.formulas += 1;
                c.text.push(' ');
                i = (j + 2).min(s.len());
                continue;
            }
            // a command name
            let mut j = i + 1;
            while j < s.len() && s[j].is_ascii_alphabetic() {
                j += 1;
            }
            if j == i + 1 {
                // \\, \%, \& and friends: an escaped character
                i += 2;
                c.text.push(' ');
                continue;
            }
            let name: String = s[i + 1..j].iter().collect();
            // \verb|...| (any delimiter): code, never text or math
            if name == "verb" {
                let mut k = j;
                if s.get(k) == Some(&'*') {
                    k += 1;
                }
                if let Some(&delim) = s.get(k) {
                    let mut e = k + 1;
                    while e < s.len() && s[e] != delim && s[e] != '\n' {
                        e += 1;
                    }
                    i = (e + 1).min(s.len());
                    c.text.push(' ');
                    continue;
                }
            }
            let after = skip_space_and_options(&s, j);

            if HEADINGS.contains(&name.as_str()) {
                if let Some((title, end)) = group(&s, after) {
                    c.start(&name, &title, line);
                    line += title.matches('\n').count() as u32;
                    i = end;
                    continue;
                }
            }
            if name == "begin" {
                if let Some((env, end)) = group(&s, after) {
                    if SKIP_ENVS.contains(&env.as_str()) {
                        let close: Vec<char> = format!("\\end{{{env}}}").chars().collect();
                        let mut k = end;
                        while k < s.len() && !s[k..].starts_with(&close) {
                            if s[k] == '\n' {
                                line += 1;
                            }
                            k += 1;
                        }
                        if !env.starts_with("verbatim")
                            && env != "lstlisting"
                            && env != "minted"
                            && env != "comment"
                            && env != "tikzpicture"
                        {
                            c.current.formulas += 1;
                        }
                        i = (k + close.len()).min(s.len());
                        c.text.push(' ');
                        continue;
                    }
                    i = end;
                    c.text.push(' ');
                    continue;
                }
            }
            if TEXT_ARGS.contains(&name.as_str()) {
                // the command goes; its argument stays as text
                i = after;
                c.text.push(' ');
                continue;
            }
            // any other command: drop it and its braced arguments
            let mut k = after;
            while let Some((arg, end)) = group(&s, k) {
                line += arg.matches('\n').count() as u32;
                k = skip_space_and_options(&s, end);
            }
            i = k;
            c.text.push(' ');
            continue;
        }
        if ch == '{' || ch == '}' {
            c.text.push(' ');
            i += 1;
            continue;
        }
        c.text.push(ch);
        i += 1;
    }
    c.flush();
    let last = c.current.clone();
    if last.level != "preamble" || last.words > 0 || last.formulas > 0 {
        c.count.sections.push(last);
    }
    c.count.words = c.count.sections.iter().map(|s| s.words).sum();
    c.count.formulas = c.count.sections.iter().map(|s| s.formulas).sum();
    c.count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_text_not_markup() {
        let c = count(
            "\\documentclass{article}\n\\usepackage{amsmath} % lots of words in a comment here\n\\begin{document}\n\\section{Intro Part}\nHello \\emph{big} world.\nSee Figure~\\ref{fig:x} and \\cite{knuth}. % not this\n\\end{document}\n",
        );
        // Hello big world See Figure and = 6
        assert_eq!(c.words, 6, "{c:#?}");
        assert_eq!(c.heading_words, 2);
        assert_eq!(c.sections.len(), 1);
        assert_eq!(c.sections[0].title, "Intro Part");
        assert_eq!(c.sections[0].line, 4);
    }

    #[test]
    fn math_is_a_formula_not_words() {
        let c = count("Let $x + y = z$ hold, and\n\\[ a b c d \\]\nthen\n\\begin{equation}\nE = mc^2\n\\end{equation}\ndone.");
        assert_eq!(c.formulas, 3);
        // Let hold and then done
        assert_eq!(c.words, 5, "{c:#?}");
    }

    #[test]
    fn splits_by_section_with_lines() {
        let c = count("\\begin{document}\nIntro words here.\n\\section{One}\na b c\n\\subsection*{Two}\nd e\n\\end{document}");
        let s: Vec<_> = c
            .sections
            .iter()
            .map(|s| (s.level.as_str(), s.title.as_str(), s.words, s.line))
            .collect();
        assert_eq!(
            s,
            vec![
                ("preamble", "", 3, 1),
                ("section", "One", 3, 3),
                ("subsection", "Two", 2, 5)
            ]
        );
        assert_eq!(c.words, 8);
    }

    #[test]
    fn escaped_percent_is_not_a_comment() {
        let c = count("Growth of 50\\% this year % but not this");
        // Growth of 50 this year
        assert_eq!(c.words, 5, "{c:#?}");
    }

    #[test]
    fn inline_verb_is_skipped_whole() {
        // the $ inside \verb must not start a formula that eats the rest
        let c = count("Type \\verb|$x$| or \\verb+$+ then more words here");
        assert_eq!(c.formulas, 0, "{c:#?}");
        // Type or then more words here
        assert_eq!(c.words, 6);
    }

    #[test]
    fn verbatim_doesnt_count() {
        let c = count("before\n\\begin{verbatim}\nlots of code words\n\\end{verbatim}\nafter");
        assert_eq!(c.words, 2);
        assert_eq!(c.formulas, 0);
    }
}
