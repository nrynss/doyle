//! Parsing the COBOL subset the story fixtures use.
//!
//! No compiler is needed: the parser reads structure, never semantics. It
//! understands the `IDENTIFICATION`, `ENVIRONMENT`, `DATA` and `PROCEDURE`
//! divisions, `COPY` names, `SELECT` files, level-01 records with level-05
//! items carrying `PIC`, and named paragraphs (optionally grouped under
//! named sections). Paragraph bodies are opaque normalized text: the parser
//! compares them, it never interprets them, so there is no control-flow or
//! data-flow analysis and no `PERFORM` graph.
//!
//! Anything outside the subset is either read as text or rejected outright,
//! never silently misread. Rejected with a [`LegacyError`](super::LegacyError)
//! naming the construct: `REDEFINES`, `OCCURS`, nested programs (a second
//! `PROGRAM-ID` or `END PROGRAM`), level 66/77/88 items, `COPY REPLACING`,
//! group items without `PIC` in a copybook, and an elementary level-01 with
//! its own `PIC`. Statements are opaque, so `EVALUATE`, `IF` branches and
//! `PERFORM` forms need no special handling — but equally get none: two
//! branches that differ only in dead code still count as changed. `FD`
//! entries, working-storage items, report-layout lines and copybook comments
//! are not elements (per `docs/legacy-ids.md`), and inline level-01 records
//! in the program file are ignored: records come from `COPY`ed copybooks.
//! A paragraph name that collides with a scope terminator (`END-IF.`) or a
//! no-op verb (`EXIT.`, `CONTINUE.`) alone on a line is out of scope: the
//! header heuristic below would misread it.

use std::collections::HashMap;
use std::path::Path;

use super::LegacyError;

/// One parsed program version: everything the index and the differ read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedProgram {
    /// `PROGRAM-ID`, uppercased.
    pub program: String,
    /// The program file's basename (`BILLBATCH.cbl`), for native locations.
    pub program_file: String,
    /// Sections and paragraphs in source order (see [`ParsedBlock`]).
    pub blocks: Vec<ParsedBlock>,
    /// Copybooks in `COPY` order.
    pub copybooks: Vec<ParsedCopybook>,
    /// `SELECT` files in source order.
    pub files: Vec<ParsedFile>,
}

/// A section header or a paragraph, in the source order met.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedBlock {
    Section(ParsedSection),
    Paragraph(ParsedParagraph),
}

/// A named section: its own body lines, if any (usually none: a section
/// that only groups unchanged paragraphs is unchanged).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSection {
    pub name: String,
    pub line: usize,
    pub body_normalized: String,
}

/// A named paragraph: its normalized body text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedParagraph {
    pub name: String,
    /// The enclosing section, if any.
    pub section: Option<String>,
    pub line: usize,
    pub body_normalized: String,
}

/// One copybook: its level-01 records in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCopybook {
    pub filename: String,
    pub records: Vec<ParsedRecord>,
}

/// One level-01 record: its fields in source order, fillers included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRecord {
    pub name: String,
    pub line: usize,
    pub fields: Vec<ParsedField>,
}

/// One copybook data item: `FILLER`s count toward the record length and
/// member offsets but never become elements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedField {
    pub name: String,
    /// The `PIC` clause verbatim, uppercased (`9(5)V99`).
    pub pic: String,
    /// Display length in characters, from [`pic_length`].
    pub length: usize,
    /// Offset from the record start, recomputed from the running lengths.
    pub offset: usize,
    pub filler: bool,
    pub line: usize,
}

/// One `SELECT` file: the `ASSIGN` literal kept with its quotes, and the
/// organization phrase (`LINE SEQUENTIAL`), empty when absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFile {
    pub name: String,
    pub assign: String,
    pub organization: String,
    pub line: usize,
}

/// Parse a program version from source text plus its copybooks.
///
/// `program_file` is the basename for native locations; `copybooks` holds
/// each `COPY`ed file in any order as `(filename, source)`. A `COPY` with
/// no matching entry fails.
pub fn parse_program(
    program_src: &str,
    program_file: &str,
    copybooks: &[(&str, &str)],
) -> Result<ParsedProgram, LegacyError> {
    let file = program_file.to_string();
    let fail = |reason: &str| LegacyError {
        file: file.clone(),
        reason: reason.to_string(),
    };
    let lines: Vec<&str> = program_src.lines().collect();
    let proc_at = lines
        .iter()
        .position(|l| {
            strip_comment(l)
                .trim_start()
                .to_uppercase()
                .starts_with("PROCEDURE DIVISION")
        })
        .ok_or_else(|| fail("no PROCEDURE DIVISION"))?;
    let head: Vec<(usize, String)> = lines[..proc_at]
        .iter()
        .enumerate()
        .map(|(i, l)| (i + 1, strip_comment(l)))
        .collect();
    let head_text = head
        .iter()
        .map(|(_, l)| l.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let program = parse_program_id(&head_text).ok_or_else(|| fail("no PROGRAM-ID"))?;
    if head_text.to_uppercase().matches("PROGRAM-ID").count() > 1 {
        return Err(fail("nested programs: more than one PROGRAM-ID"));
    }
    if head_text.to_uppercase().contains("END PROGRAM") {
        return Err(fail("nested programs: END PROGRAM"));
    }
    let files = parse_selects(&head_text).map_err(|reason| fail(&reason))?;
    let wanted = parse_copies(&head_text).map_err(|reason| fail(&reason))?;
    let by_name: HashMap<String, &str> = copybooks
        .iter()
        .map(|(name, src)| (name.to_uppercase(), *src))
        .collect();
    let mut books = Vec::new();
    for name in &wanted {
        let src = by_name
            .get(&name.to_uppercase())
            .ok_or_else(|| fail(&format!("COPY {name}: no such copybook was supplied")))?;
        books.push(ParsedCopybook {
            filename: name.clone(),
            records: parse_copybook(name, src).map_err(|reason| fail(&reason))?,
        });
    }
    let blocks = parse_procedure(&lines, proc_at).map_err(|reason| fail(&reason))?;

    Ok(ParsedProgram {
        program,
        program_file: program_file.to_string(),
        blocks,
        copybooks: books,
        files,
    })
}

/// Load a program version from a `.cbl` path: `COPY` names resolve to
/// sibling files of the program.
pub fn load_program(path: &Path) -> Result<ParsedProgram, LegacyError> {
    let display = path.display().to_string();
    let fail = |reason: String| LegacyError {
        file: display.clone(),
        reason,
    };
    let src = std::fs::read_to_string(path).map_err(|e| fail(format!("cannot read: {e}")))?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let program_file = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("program.cbl")
        .to_string();
    // The COPY list needs the head text before the copybooks can load.
    let lines: Vec<&str> = src.lines().collect();
    let proc_at = lines
        .iter()
        .position(|l| {
            strip_comment(l)
                .trim_start()
                .to_uppercase()
                .starts_with("PROCEDURE DIVISION")
        })
        .ok_or_else(|| fail("no PROCEDURE DIVISION".to_string()))?;
    let head_text = lines[..proc_at]
        .iter()
        .map(|l| strip_comment(l))
        .collect::<Vec<_>>()
        .join("\n");
    let wanted = parse_copies(&head_text).map_err(fail)?;
    let mut books = Vec::new();
    for name in &wanted {
        let text = std::fs::read_to_string(dir.join(name))
            .map_err(|e| fail(format!("COPY {name}: cannot read: {e}")))?;
        books.push((name.clone(), text));
    }
    let refs: Vec<(&str, &str)> = books
        .iter()
        .map(|(n, s)| (n.as_str(), s.as_str()))
        .collect();
    parse_program(&src, &program_file, &refs)
}

/// Cut a `*>` comment: free format starts a comment anywhere outside a
/// string literal. Handles doubled quotes inside strings.
fn strip_comment(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut string: Option<char> = None;
    while let Some(c) = chars.next() {
        if let Some(q) = string {
            out.push(c);
            if c == q {
                if chars.peek() == Some(&q) {
                    out.push(chars.next().expect("peeked"));
                } else {
                    string = None;
                }
            }
        } else if c == '"' || c == '\'' {
            string = Some(c);
            out.push(c);
        } else if c == '*' && chars.peek() == Some(&'>') {
            break;
        } else {
            out.push(c);
        }
    }
    out
}

/// Split text into sentences: a `.` followed by whitespace or the end.
/// Interior decimals (`9.99`) never terminate.
fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        current.push(c);
        if c == '.' && chars.peek().is_none_or(|n| n.is_whitespace()) {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                out.push(trimmed);
            }
            current = String::new();
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        out.push(trimmed);
    }
    out
}

/// Collapse every whitespace run to one space, and uppercase: COBOL is
/// case-insensitive and layout-free, so reformatting never changes this.
fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

fn parse_program_id(head_text: &str) -> Option<String> {
    let sentences = sentences(head_text);
    for (i, sentence) in sentences.iter().enumerate() {
        let upper = sentence.to_uppercase();
        if upper == "PROGRAM-ID" || upper == "PROGRAM-ID." {
            // `PROGRAM-ID.` ends its own sentence; the name is next.
            let name = sentences.get(i + 1)?;
            return Some(name.trim_end_matches('.').trim().to_uppercase());
        }
        if upper.starts_with("PROGRAM-ID") {
            let rest = sentence["PROGRAM-ID".len()..]
                .trim()
                .trim_start_matches('.')
                .trim()
                .trim_end_matches('.')
                .trim();
            let name = rest.split_whitespace().next()?;
            if name.is_empty() {
                return None;
            }
            return Some(name.to_uppercase());
        }
    }
    None
}

fn parse_copies(head_text: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let sentences = sentences(head_text);
    for (i, sentence) in sentences.iter().enumerate() {
        let upper = sentence.to_uppercase();
        if upper == "COPY" {
            // `COPY.` ends its own sentence; the name is next.
            let name = sentences
                .get(i + 1)
                .ok_or_else(|| "COPY with no file name is outside the subset".to_string())?;
            out.push(name.trim_end_matches('.').trim_matches('"').to_string());
            continue;
        }
        if !upper.starts_with("COPY") {
            continue;
        }
        if upper.contains("REPLACING") {
            return Err("COPY REPLACING is outside the subset".to_string());
        }
        let rest = sentence["COPY".len()..]
            .trim()
            .trim_end_matches('.')
            .trim()
            .trim_matches('"')
            .trim();
        let name = rest
            .split_whitespace()
            .next()
            .ok_or_else(|| "COPY with no file name is outside the subset".to_string())?;
        if name.is_empty() {
            return Err("COPY with no file name is outside the subset".to_string());
        }
        out.push(name.to_string());
    }
    Ok(out)
}

fn parse_selects(head_text: &str) -> Result<Vec<ParsedFile>, String> {
    let mut out = Vec::new();
    let mut seen: HashMap<String, ()> = HashMap::new();
    for sentence in sentences(head_text) {
        let upper = sentence.to_uppercase();
        if !upper.starts_with("SELECT") {
            continue;
        }
        let tokens: Vec<&str> = sentence.split_whitespace().collect();
        if tokens.len() < 2 {
            return Err("SELECT with no file name is outside the subset".to_string());
        }
        let name = tokens[1].trim_end_matches('.').to_uppercase();
        if seen.contains_key(&name) {
            return Err(format!("duplicate SELECT {name}"));
        }
        seen.insert(name.clone(), ());
        let mut assign = String::new();
        let mut organization = String::new();
        let mut i = 2;
        let uppers: Vec<String> = tokens.iter().map(|t| t.to_uppercase()).collect();
        while i < tokens.len() {
            match uppers[i].as_str() {
                "ASSIGN" => {
                    if uppers.get(i + 1).is_some_and(|t| t == "TO") {
                        i += 2;
                    } else {
                        i += 1;
                    }
                    if let Some(lit) = tokens.get(i) {
                        assign = lit.trim_end_matches('.').to_string();
                    }
                    i += 1;
                }
                "ORGANIZATION" => {
                    if uppers.get(i + 1).is_some_and(|t| t == "IS") {
                        i += 2;
                    } else {
                        i += 1;
                    }
                    let mut words = Vec::new();
                    while i < tokens.len() {
                        words.push(tokens[i].trim_end_matches('.'));
                        i += 1;
                    }
                    organization = words.join(" ").to_uppercase();
                }
                _ => {
                    i += 1;
                }
            }
        }
        out.push(ParsedFile {
            name,
            assign,
            organization,
            line: 0,
        });
    }
    // Line numbers need the original lines: fill them in second.
    let mut by_name: HashMap<String, usize> = HashMap::new();
    for (index, entry) in out.iter().enumerate() {
        by_name.insert(entry.name.clone(), index);
    }
    for (i, raw) in head_text.lines().enumerate() {
        let upper = raw.to_uppercase();
        if let Some(at) = upper.find("SELECT") {
            let after = raw[at + "SELECT".len()..].trim();
            let name = after
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches('.')
                .to_uppercase();
            if let Some(index) = by_name.get(&name)
                && out[*index].line == 0
            {
                out[*index].line = i + 1;
            }
        }
    }
    Ok(out)
}

/// Display length of a `PIC` clause: repetitions expand, `V`/`S`/`P` take
/// no character, every other picture character takes one.
pub fn pic_length(pic: &str) -> Result<usize, String> {
    let compact: String = pic
        .to_uppercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if compact.is_empty() {
        return Err("empty PIC".to_string());
    }
    let mut expanded = String::new();
    let mut chars = compact.chars().peekable();
    let mut last = '\0';
    while let Some(c) = chars.next() {
        if c == '(' {
            let mut digits = String::new();
            for d in chars.by_ref() {
                if d == ')' {
                    break;
                }
                digits.push(d);
            }
            let n: usize = digits
                .parse()
                .map_err(|_| format!("bad PIC repetition in {pic}"))?;
            if n == 0 || last == '\0' {
                return Err(format!("bad PIC repetition in {pic}"));
            }
            for _ in 1..n {
                expanded.push(last);
            }
        } else {
            expanded.push(c);
            last = c;
        }
    }
    Ok(expanded
        .chars()
        .filter(|c| !matches!(c, 'V' | 'S' | 'P'))
        .count())
}

fn parse_copybook(filename: &str, src: &str) -> Result<Vec<ParsedRecord>, String> {
    let stripped: Vec<String> = src.lines().map(strip_comment).collect();
    let text = stripped.join("\n");
    let upper_probe = text.to_uppercase();
    for banned in ["REDEFINES", "OCCURS", "END PROGRAM"] {
        if upper_probe.contains(banned) {
            return Err(format!("{filename}: {banned} is outside the subset"));
        }
    }
    let mut records = Vec::new();
    let mut current: Option<ParsedRecord> = None;
    let mut offset = 0;
    for sentence in sentences(&text) {
        let tokens: Vec<&str> = sentence.split_whitespace().collect();
        if tokens.len() < 2 {
            continue;
        }
        let level: u32 = tokens[0]
            .parse()
            .map_err(|_| format!("{}: cannot read level in {sentence:?}", filename))?;
        if matches!(level, 66 | 77 | 88) {
            return Err(format!("{filename}: level {level} is outside the subset"));
        }
        let name = tokens[1].trim_end_matches('.').to_uppercase();
        if level == 1 {
            if let Some(done) = current.take() {
                records.push(done);
            }
            if sentence.to_uppercase().contains("PIC") {
                return Err(format!(
                    "{filename}: elementary level-01 {name} is outside the subset"
                ));
            }
            if records.iter().any(|r: &ParsedRecord| r.name == name) {
                return Err(format!("{filename}: duplicate record {name}"));
            }
            current = Some(ParsedRecord {
                name,
                line: 0,
                fields: Vec::new(),
            });
            offset = 0;
            continue;
        }
        let record = current
            .as_mut()
            .ok_or_else(|| format!("{filename}: level {level} outside any record"))?;
        if record.fields.iter().any(|f| f.name == name && !f.filler) {
            return Err(format!("{filename}: duplicate field {name}"));
        }
        let pic_at = tokens
            .iter()
            .position(|t| t.to_uppercase() == "PIC")
            .ok_or_else(|| {
                format!("{filename}: group item {name} without PIC is outside the subset")
            })?;
        let pic = tokens
            .get(pic_at + 1)
            .ok_or_else(|| format!("{filename}: {name} has PIC with no picture"))?
            .trim_end_matches('.')
            .to_uppercase();
        let length = pic_length(&pic).map_err(|e| format!("{filename}: {name}: {e}"))?;
        let filler = name == "FILLER";
        record.fields.push(ParsedField {
            name,
            pic,
            length,
            offset,
            filler,
            line: 0,
        });
        offset += length;
    }
    if let Some(done) = current.take() {
        records.push(done);
    }
    if records.is_empty() {
        return Err(format!("{filename}: no level-01 record"));
    }
    // Copybook line numbers: the sentence of each header and field.
    let mut record_at: HashMap<String, usize> = HashMap::new();
    let mut field_at: HashMap<String, usize> = HashMap::new();
    for (i, raw) in src.lines().enumerate() {
        let clean = strip_comment(raw).trim().to_uppercase();
        let words: Vec<&str> = clean.split_whitespace().collect();
        if words.len() >= 2 && words[0].parse::<u32>().is_ok() {
            let item = words[1].trim_end_matches('.').to_string();
            if words[0] == "01" {
                record_at.entry(item).or_insert(i + 1);
            } else {
                field_at.entry(item).or_insert(i + 1);
            }
        }
    }
    for record in &mut records {
        record.line = record_at.get(&record.name).copied().unwrap_or(0);
        for field in &mut record.fields {
            field.line = field_at.get(&field.name).copied().unwrap_or(0);
        }
    }
    Ok(records)
}

/// Words that may end with a period alone on a line without starting a
/// paragraph: scope terminators and no-op verbs. A paragraph name colliding
/// with one of these is outside the subset.
fn is_reserved_header(name: &str) -> bool {
    name.starts_with("END-") || matches!(name, "EXIT" | "GOBACK" | "STOP" | "CONTINUE")
}

/// A paragraph or section header: `NAME.` or `NAME SECTION.` alone on the
/// line (after comment stripping), uppercased, starting with a letter.
fn header_of(line: &str) -> Option<(String, bool)> {
    let trimmed = line.trim();
    let core = trimmed.strip_suffix('.')?.trim();
    if core.is_empty() || core.contains('.') {
        return None;
    }
    let parts: Vec<&str> = core.split_whitespace().collect();
    let (name, is_section) = match parts.as_slice() {
        [name] => (*name, false),
        [name, section] if section.to_uppercase() == "SECTION" => (*name, true),
        _ => return None,
    };
    if !name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    let upper = name.to_uppercase();
    if is_reserved_header(&upper) {
        return None;
    }
    Some((upper, is_section))
}

fn parse_procedure(lines: &[&str], proc_at: usize) -> Result<Vec<ParsedBlock>, String> {
    let mut blocks = Vec::new();
    let mut current_para: Option<ParsedParagraph> = None;
    let mut current_section: Option<String> = None;
    let mut section_body: Vec<String> = Vec::new();
    let mut section_line = 0;
    let mut seen_paras: HashMap<String, ()> = HashMap::new();
    let mut seen_sections: HashMap<String, ()> = HashMap::new();

    let flush_para = |current_para: &mut Option<ParsedParagraph>, blocks: &mut Vec<ParsedBlock>| {
        if let Some(mut para) = current_para.take() {
            para.body_normalized = normalize(&para.body_normalized);
            blocks.push(ParsedBlock::Paragraph(para));
        }
    };
    let flush_section = |current_section: &mut Option<String>,
                         section_body: &mut Vec<String>,
                         section_line: &mut usize,
                         blocks: &mut Vec<ParsedBlock>| {
        if let Some(name) = current_section.take() {
            blocks.push(ParsedBlock::Section(ParsedSection {
                name,
                line: *section_line,
                body_normalized: normalize(&section_body.join("\n")),
            }));
            section_body.clear();
        }
    };

    for (i, raw) in lines.iter().enumerate().skip(proc_at + 1) {
        let clean = strip_comment(raw);
        if clean.trim().is_empty() {
            continue;
        }
        let upper = clean.to_uppercase();
        if let Some((name, is_section)) = header_of(&upper) {
            flush_para(&mut current_para, &mut blocks);
            if is_section {
                flush_section(
                    &mut current_section,
                    &mut section_body,
                    &mut section_line,
                    &mut blocks,
                );
                if seen_sections.contains_key(&name) {
                    return Err(format!("duplicate section {name}"));
                }
                seen_sections.insert(name.clone(), ());
                current_section = Some(name);
                section_line = i + 1;
            } else {
                if seen_paras.contains_key(&name) {
                    return Err(format!("duplicate paragraph {name}"));
                }
                seen_paras.insert(name.clone(), ());
                current_para = Some(ParsedParagraph {
                    name,
                    section: current_section.clone(),
                    line: i + 1,
                    body_normalized: String::new(),
                });
            }
            continue;
        }
        if let Some(para) = current_para.as_mut() {
            if !para.body_normalized.is_empty() {
                para.body_normalized.push('\n');
            }
            para.body_normalized.push_str(clean.trim());
        } else if current_section.is_some() {
            section_body.push(clean.trim().to_string());
        } else {
            return Err(format!(
                "line {}: statements outside any paragraph are outside the subset",
                i + 1
            ));
        }
    }
    flush_para(&mut current_para, &mut blocks);
    flush_section(
        &mut current_section,
        &mut section_body,
        &mut section_line,
        &mut blocks,
    );
    if blocks.is_empty() {
        return Err("PROCEDURE DIVISION holds no paragraphs".to_string());
    }
    Ok(blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1_PROGRAM: &str = include_str!("../../../testdata/legacy/story/v1/BILLBATCH.cbl");
    const V1_COPYBOOK: &str = include_str!("../../../testdata/legacy/story/v1/patient.cpy");

    fn v1() -> ParsedProgram {
        parse_program(V1_PROGRAM, "BILLBATCH.cbl", &[("patient.cpy", V1_COPYBOOK)])
            .expect("story v1 parses")
    }

    #[test]
    fn pic_lengths_cover_the_subset() {
        for (pic, want) in [
            ("9(6)", 6),
            ("X(20)", 20),
            ("X", 1),
            ("9(5)V99", 7),
            ("9(4)", 4),
            ("V999", 3),
            ("S9(4)", 4),
            ("9(7)V99", 9),
            ("X(72)", 72),
            ("$$$,$$9.99", 10),
        ] {
            assert_eq!(pic_length(pic).expect("valid PIC"), want, "PIC {pic}");
        }
        assert!(pic_length("").is_err());
        assert!(pic_length("9()").is_err());
    }

    #[test]
    fn story_v1_parses_to_its_paragraphs_files_and_record() {
        let parsed = v1();
        assert_eq!(parsed.program, "BILLBATCH");
        let paras: Vec<&str> = parsed
            .blocks
            .iter()
            .filter_map(|b| match b {
                ParsedBlock::Paragraph(p) => Some(p.name.as_str()),
                ParsedBlock::Section(_) => None,
            })
            .collect();
        assert_eq!(paras.len(), 16, "sixteen paragraphs: {paras:?}");
        assert!(paras.contains(&"CALC-LATE-FEE"));
        assert!(paras.contains(&"LOG-EXEMPT"));
        // Paragraph bodies are opaque normalized text: the fee rate survives.
        let calc = parsed
            .blocks
            .iter()
            .find_map(|b| match b {
                ParsedBlock::Paragraph(p) if p.name == "CALC-LATE-FEE" => Some(p),
                _ => None,
            })
            .expect("CALC-LATE-FEE");
        assert!(
            calc.body_normalized.contains("BALANCE * WS-FEE-RATE"),
            "body kept: {}",
            calc.body_normalized
        );
        // SELECT files with ASSIGN and organization.
        assert_eq!(parsed.files.len(), 2);
        assert_eq!(parsed.files[0].name, "ACCT-FILE");
        assert_eq!(parsed.files[0].assign, "\"patients.dat\"");
        assert_eq!(parsed.files[0].organization, "LINE SEQUENTIAL");
        // One copybook, one record, four fields, 34 chars.
        assert_eq!(parsed.copybooks.len(), 1);
        let record = &parsed.copybooks[0].records[0];
        assert_eq!(record.name, "PATIENT-RECORD");
        let total: usize = record.fields.iter().map(|f| f.length).sum();
        assert_eq!(total, 34);
        let offsets: Vec<usize> = record.fields.iter().map(|f| f.offset).collect();
        assert_eq!(offsets, [0, 6, 26, 33], "offsets recomputed, not copied");
    }

    #[test]
    fn terminator_lines_are_never_paragraphs() {
        // END-IF. and END-READ. close paragraphs in the story; the header
        // heuristic must not promote them.
        let parsed = v1();
        for block in &parsed.blocks {
            if let ParsedBlock::Paragraph(p) = block {
                assert!(
                    !p.name.starts_with("END-"),
                    "terminator became a paragraph: {}",
                    p.name
                );
            }
        }
    }

    #[test]
    fn unsupported_constructs_fail_by_name() {
        let bad_copy = "01 R.\n    05 A PIC X REDEFINES B.\n";
        let err = parse_program(V1_PROGRAM, "BILLBATCH.cbl", &[("patient.cpy", bad_copy)])
            .expect_err("REDEFINES fails");
        assert!(err.reason.contains("REDEFINES"), "names it: {}", err.reason);
        let occurs = "01 R.\n    05 A PIC X(4) OCCURS 4 TIMES.\n";
        let err = parse_program(V1_PROGRAM, "BILLBATCH.cbl", &[("patient.cpy", occurs)])
            .expect_err("OCCURS fails");
        assert!(err.reason.contains("OCCURS"), "names it: {}", err.reason);
        let nested = V1_PROGRAM.replacen("PROGRAM-ID.", "PROGRAM-ID. BILLBATCH.\nPROGRAM-ID.", 1);
        let err = parse_program(&nested, "BILLBATCH.cbl", &[("patient.cpy", V1_COPYBOOK)])
            .expect_err("nested program fails");
        assert!(
            err.reason.contains("nested programs"),
            "names it: {}",
            err.reason
        );
        let cond = "01 R.\n    05 A PIC X.\n    88 B VALUE \"X\".\n";
        let err = parse_program(V1_PROGRAM, "BILLBATCH.cbl", &[("patient.cpy", cond)])
            .expect_err("level 88 fails");
        assert!(err.reason.contains("88"), "names it: {}", err.reason);
        let replacing = V1_PROGRAM.replacen(
            "COPY \"patient.cpy\".",
            "COPY \"patient.cpy\" REPLACING ==A== BY ==B==.",
            1,
        );
        let err = parse_program(&replacing, "BILLBATCH.cbl", &[("patient.cpy", V1_COPYBOOK)])
            .expect_err("COPY REPLACING fails");
        assert!(err.reason.contains("REPLACING"), "names it: {}", err.reason);
    }
}
