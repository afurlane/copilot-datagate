use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, PartialEq, Eq)]
struct FileCoverage {
    lines: BTreeMap<u32, u64>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let input = args
        .next()
        .ok_or_else(|| "usage: lcov_to_sonar <input.lcov> <output.xml>".to_string())?;
    let output = args
        .next()
        .ok_or_else(|| "usage: lcov_to_sonar <input.lcov> <output.xml>".to_string())?;
    if args.next().is_some() {
        return Err("usage: lcov_to_sonar <input.lcov> <output.xml>".to_string());
    }

    let raw = fs::read_to_string(&input)
        .map_err(|err| format!("failed reading lcov report `{input}`: {err}"))?;
    let coverage = parse_lcov(&raw)?;
    let xml = to_sonar_xml(&coverage)?;

    let output_path = Path::new(&output);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed creating output directory `{}`: {err}",
                parent.display()
            )
        })?;
    }

    fs::write(output_path, xml)
        .map_err(|err| format!("failed writing sonar report `{output}`: {err}"))?;
    Ok(())
}

fn parse_lcov(raw: &str) -> Result<BTreeMap<PathBuf, FileCoverage>, String> {
    let cwd = env::current_dir().map_err(|err| format!("failed reading current dir: {err}"))?;
    let mut result: BTreeMap<PathBuf, FileCoverage> = BTreeMap::new();

    let mut current_file: Option<PathBuf> = None;
    for (line_number, line) in raw.lines().enumerate() {
        if let Some(path) = line.strip_prefix("SF:") {
            let normalized = normalize_path(path.trim(), &cwd);
            current_file = Some(normalized.clone());
            result.entry(normalized).or_default();
            continue;
        }

        if line == "end_of_record" {
            current_file = None;
            continue;
        }

        let Some(file_path) = &current_file else {
            continue;
        };

        if let Some(record) = line.strip_prefix("DA:") {
            let (line_no_raw, hits_raw) = record.split_once(',').ok_or_else(|| {
                format!("invalid DA record at line {}: `{line}`", line_number + 1)
            })?;
            let line_no = line_no_raw.parse::<u32>().map_err(|_| {
                format!(
                    "invalid DA line number at line {}: `{line_no_raw}`",
                    line_number + 1
                )
            })?;
            let hits = hits_raw.parse::<u64>().map_err(|_| {
                format!(
                    "invalid DA hit count at line {}: `{hits_raw}`",
                    line_number + 1
                )
            })?;

            let entry = result
                .get_mut(file_path)
                .ok_or_else(|| "missing current file coverage entry".to_string())?;
            entry
                .lines
                .entry(line_no)
                .and_modify(|existing| *existing = (*existing).max(hits))
                .or_insert(hits);
        }
    }

    Ok(result)
}

fn normalize_path(path: &str, cwd: &Path) -> PathBuf {
    let candidate = PathBuf::from(path);
    if let Ok(relative) = candidate.strip_prefix(cwd) {
        relative.to_path_buf()
    } else {
        candidate
    }
}

fn to_sonar_xml(coverage: &BTreeMap<PathBuf, FileCoverage>) -> Result<String, String> {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');
    xml.push_str(r#"<coverage version="1">"#);
    xml.push('\n');

    for (path, file_coverage) in coverage {
        let path = path
            .to_str()
            .ok_or_else(|| format!("non utf-8 path in coverage: `{}`", path.display()))?;
        xml.push_str("  <file path=\"");
        xml.push_str(&escape_xml(path));
        xml.push_str("\">\n");

        for (line_number, hits) in &file_coverage.lines {
            let covered = if *hits > 0 { "true" } else { "false" };
            xml.push_str(&format!(
                "    <lineToCover lineNumber=\"{}\" covered=\"{}\"/>\n",
                line_number, covered
            ));
        }

        xml.push_str("  </file>\n");
    }

    xml.push_str("</coverage>\n");
    Ok(xml)
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lcov_and_keeps_max_hits_for_duplicate_lines() {
        let raw = "SF:src/lib.rs\nDA:10,0\nDA:10,2\nDA:11,1\nend_of_record\n";
        let coverage = parse_lcov(raw).expect("valid lcov");

        let file = coverage
            .get(&PathBuf::from("src/lib.rs"))
            .expect("coverage for file");
        assert_eq!(file.lines.get(&10), Some(&2));
        assert_eq!(file.lines.get(&11), Some(&1));
    }

    #[test]
    fn renders_generic_sonar_xml() {
        let mut coverage = BTreeMap::new();
        coverage.insert(
            PathBuf::from("src/main.rs"),
            FileCoverage {
                lines: BTreeMap::from([(1, 1), (2, 0)]),
            },
        );

        let xml = to_sonar_xml(&coverage).expect("xml output");
        assert!(xml.contains(r#"<coverage version="1">"#));
        assert!(xml.contains(r#"<file path="src/main.rs">"#));
        assert!(xml.contains(r#"lineNumber="1" covered="true""#));
        assert!(xml.contains(r#"lineNumber="2" covered="false""#));
    }
}
