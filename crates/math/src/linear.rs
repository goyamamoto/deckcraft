//! The linear (one-line, plain text) form of an equation: what readers without equation support
//! see, what Find searches, what the PPTX fallback text holds.

use crate::ast::{Math, Node};

/// `x=(-b±√(b^2-4ac))/2a`
pub fn linear(m: &Math) -> String {
    let mut out = String::new();
    write_row(m, &mut out, 0);
    out
}

fn write_row(m: &Math, out: &mut String, depth: usize) {
    if depth > 100 {
        return;
    }
    for n in m {
        write_node(n, out, depth + 1);
    }
}

/// A row in parentheses unless it is a single run or a lone structure.
fn group(m: &Math, out: &mut String, depth: usize) {
    let simple = match m.as_slice() {
        [Node::Run { text, .. }] => text.chars().count() == 1 || text.chars().all(|c| c.is_alphanumeric()),
        [Node::Delim { .. }] => true,
        _ => false,
    };
    if simple {
        write_row(m, out, depth);
    } else {
        out.push('(');
        write_row(m, out, depth);
        out.push(')');
    }
}

pub(crate) fn linear_node(n: &Node) -> String {
    let mut s = String::new();
    write_node(n, &mut s, 0);
    s
}

fn write_node(n: &Node, out: &mut String, depth: usize) {
    if depth > 100 {
        return;
    }
    match n {
        Node::Run { text, .. } => out.push_str(text),
        Node::Frac { num, den, bar } => {
            group(num, out, depth);
            out.push(if *bar { '/' } else { '¦' });
            group(den, out, depth);
        }
        Node::Script { base, sub, sup } => {
            group(base, out, depth);
            if let Some(s) = sub {
                out.push('_');
                group(s, out, depth);
            }
            if let Some(s) = sup {
                out.push('^');
                group(s, out, depth);
            }
        }
        Node::Rad { index, body } => match index {
            Some(i) => {
                out.push('√');
                out.push('(');
                write_row(i, out, depth);
                out.push('&');
                write_row(body, out, depth);
                out.push(')');
            }
            None => {
                out.push('√');
                group(body, out, depth);
            }
        },
        Node::Nary { op, sub, sup, body, .. } => {
            out.push(*op);
            if let Some(s) = sub {
                out.push('_');
                group(s, out, depth);
            }
            if let Some(s) = sup {
                out.push('^');
                group(s, out, depth);
            }
            if !body.is_empty() {
                out.push('▒');
                write_row(body, out, depth);
            }
        }
        Node::Delim { open, close, sep, items } => {
            if let Some(c) = open {
                out.push(*c);
            }
            for (k, item) in items.iter().enumerate() {
                if k > 0 {
                    out.push(sep.unwrap_or('|'));
                }
                write_row(item, out, depth);
            }
            if let Some(c) = close {
                out.push(*c);
            }
        }
        Node::Acc { chr, body } => {
            group(body, out, depth);
            out.push(*chr);
        }
        Node::Bar { body, top } => {
            group(body, out, depth);
            out.push(if *top { '\u{305}' } else { '\u{332}' });
        }
        Node::Func { name, arg } => {
            write_row(name, out, depth);
            out.push('\u{2061}');
            group(arg, out, depth);
        }
        Node::Lim { base, lim, above } => {
            group(base, out, depth);
            out.push(if *above { '^' } else { '_' });
            group(lim, out, depth);
        }
        Node::Matrix { rows, .. } => {
            out.push('■');
            out.push('(');
            for (ri, r) in rows.iter().enumerate() {
                if ri > 0 {
                    out.push('@');
                }
                for (ci, c) in r.iter().enumerate() {
                    if ci > 0 {
                        out.push('&');
                    }
                    write_row(c, out, depth);
                }
            }
            out.push(')');
        }
        Node::EqArr { rows } => {
            for (ri, r) in rows.iter().enumerate() {
                if ri > 0 {
                    out.push_str("; ");
                }
                write_row(r, out, depth);
            }
        }
        Node::AlignMark => {}
        Node::GroupChr { body, .. } | Node::Boxed { body } => write_row(body, out, depth),
        Node::Space(em) => {
            if *em >= 0.3 {
                out.push(' ');
            }
        }
    }
}
