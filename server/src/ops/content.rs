use std::path::Path;
use std::sync::Arc;

use serde::Serialize;

use crate::index::file_entry::Language;
use crate::index::file_tree::FileTree;
use crate::symbols::{parser, queries};

use crate::server::state::{FileCache, ParseCache};

#[derive(Debug, Serialize)]
pub struct PeekResponse {
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
    pub total_lines: usize,
    pub content: String,
}

pub fn peek(
    root: &Path,
    file_tree: &Arc<FileTree>,
    file_cache: &Arc<FileCache>,
    file: &str,
    start: usize,
    end: usize,
) -> Result<PeekResponse, String> {
    if file_tree.get(file).is_none() {
        return Err(format!("File '{}' not found in index", file));
    }

    let abs_path = root.join(file);
    let source = file_cache
        .get_or_read(&abs_path, file)
        .map_err(|e| e.to_string())?;

    let lines: Vec<&str> = source.lines().collect();
    let total_lines = lines.len();
    let start = start.min(total_lines);
    let end = end.min(total_lines);

    let content: String = lines[start..end]
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:>6} │ {}", start + i + 1, line))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(PeekResponse {
        file: file.to_string(),
        start_line: start + 1,
        end_line: end,
        total_lines,
        content,
    })
}

#[derive(Debug, Serialize)]
pub struct GrepResponse {
    pub pattern: String,
    pub matches: Vec<GrepMatch>,
    pub total_matches: usize,
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct GrepMatch {
    pub file: String,
    pub line: usize,
    pub text: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

/// Scope filter for grep: restrict matches to code only (skip comments/strings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrepScope {
    /// Match anywhere (default behavior).
    All,
    /// Only match in code — skip matches inside comment and string AST nodes.
    Code,
}

impl std::str::FromStr for GrepScope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "all" => Ok(GrepScope::All),
            "code" => Ok(GrepScope::Code),
            _ => Err(format!("Unknown grep scope: '{}'. Valid: all, code", s)),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn grep_with_scope(
    root: &Path,
    file_tree: &Arc<FileTree>,
    file_cache: &Arc<FileCache>,
    parse_cache: &Arc<ParseCache>,
    pattern: &str,
    max_matches: usize,
    context_lines: usize,
    scope: GrepScope,
) -> Result<GrepResponse, String> {
    let re = regex::RegexBuilder::new(pattern)
        .size_limit(1_000_000)
        .build()
        .map_err(|e| format!("Invalid regex: {}", e))?;

    let mut matches = Vec::new();
    let mut total = 0;

    let mut paths: Vec<(String, Language)> = file_tree
        .files
        .iter()
        .map(|e| (e.key().clone(), e.value().language))
        .collect();
    paths.sort_by(|a, b| a.0.cmp(&b.0));

    for (rel_path, language) in &paths {
        let abs_path = root.join(rel_path);
        let source = match file_cache.get_or_read(&abs_path, rel_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // For scope=code, build a set of byte ranges that are inside comments/strings
        let excluded_ranges = if scope == GrepScope::Code && language.has_tree_sitter_support() {
            compute_non_code_ranges(&source, rel_path, *language, parse_cache)
        } else {
            Vec::new()
        };

        let lines: Vec<&str> = source.lines().collect();

        // Pre-compute line byte offsets for scope filtering
        let line_offsets: Vec<usize> = if scope == GrepScope::Code {
            let mut offsets = Vec::with_capacity(lines.len());
            let mut offset = 0;
            for line in &lines {
                offsets.push(offset);
                offset += line.len() + 1; // +1 for newline
            }
            offsets
        } else {
            Vec::new()
        };

        for (i, line) in lines.iter().enumerate() {
            if re.is_match(line) {
                // If scope=code, check that the match byte offset is not inside an excluded range
                if scope == GrepScope::Code && !excluded_ranges.is_empty() {
                    let line_start = line_offsets[i];
                    // Find where in the line the regex matched
                    if let Some(m) = re.find(line) {
                        let match_byte = line_start + m.start();
                        if is_in_excluded_range(match_byte, &excluded_ranges) {
                            continue;
                        }
                    }
                }

                total += 1;
                if matches.len() < max_matches {
                    let ctx_start = i.saturating_sub(context_lines);
                    let ctx_end = (i + context_lines + 1).min(lines.len());

                    let context_before: Vec<String> = lines[ctx_start..i]
                        .iter()
                        .map(|l| l.to_string())
                        .collect();
                    let context_after: Vec<String> = lines[(i + 1)..ctx_end]
                        .iter()
                        .map(|l| l.to_string())
                        .collect();

                    matches.push(GrepMatch {
                        file: rel_path.clone(),
                        line: i + 1,
                        text: line.to_string(),
                        context_before,
                        context_after,
                    });
                }
            }
        }
    }

    Ok(GrepResponse {
        pattern: pattern.to_string(),
        matches,
        total_matches: total,
        truncated: total > max_matches,
    })
}

/// Compute byte ranges of comment and string nodes using tree-sitter.
fn compute_non_code_ranges(
    source: &str,
    rel_path: &str,
    language: Language,
    parse_cache: &ParseCache,
) -> Vec<(usize, usize)> {
    use tree_sitter::StreamingIterator;

    let config = match queries::get_language_config(language) {
        Some(c) => c,
        None => return Vec::new(),
    };

    let tree = match parser::get_parse_tree(rel_path, source, language, parse_cache) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    // Query for comment and string nodes
    let query_str = match language {
        Language::Rust => r#"
            (line_comment) @skip
            (block_comment) @skip
            (string_literal) @skip
            (raw_string_literal) @skip
        "#,
        Language::Python => r#"
            (comment) @skip
            (string) @skip
        "#,
        Language::TypeScript | Language::JavaScript => r#"
            (comment) @skip
            (string) @skip
            (template_string) @skip
        "#,
        Language::Go => r#"
            (comment) @skip
            (raw_string_literal) @skip
            (interpreted_string_literal) @skip
        "#,
        Language::Java => r#"
            (line_comment) @skip
            (block_comment) @skip
            (string_literal) @skip
        "#,
        Language::Scala => r#"
            (comment) @skip
            (block_comment) @skip
            (string) @skip
            (interpolated_string_expression) @skip
        "#,
        _ => return Vec::new(),
    };

    let query = match tree_sitter::Query::new(&config.language, query_str) {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    let mut ranges = Vec::new();

    while let Some(m) = matches.next() {
        for cap in m.captures {
            ranges.push((cap.node.start_byte(), cap.node.end_byte()));
        }
    }

    // Sort by start byte for binary search
    ranges.sort_by_key(|r| r.0);
    ranges
}

fn is_in_excluded_range(byte_offset: usize, ranges: &[(usize, usize)]) -> bool {
    // Binary search for efficiency
    ranges
        .binary_search_by(|&(start, end)| {
            if byte_offset < start {
                std::cmp::Ordering::Greater
            } else if byte_offset >= end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

#[derive(Debug, Serialize)]
pub struct ChunkIndicesResponse {
    pub file: String,
    pub total_bytes: usize,
    pub chunk_size: usize,
    pub overlap: usize,
    pub chunks: Vec<ChunkInfo>,
}

#[derive(Debug, Serialize)]
pub struct ChunkInfo {
    pub index: usize,
    pub start: usize,
    pub end: usize,
}

pub fn chunk_indices(
    root: &Path,
    file_tree: &Arc<FileTree>,
    file_cache: &Arc<FileCache>,
    file: &str,
    size: usize,
    overlap: usize,
) -> Result<ChunkIndicesResponse, String> {
    if size == 0 {
        return Err("Chunk size must be > 0".to_string());
    }
    if overlap >= size {
        return Err("Overlap must be < chunk size".to_string());
    }
    if file_tree.get(file).is_none() {
        return Err(format!("File '{}' not found in index", file));
    }

    let abs_path = root.join(file);
    let source = file_cache
        .get_or_read(&abs_path, file)
        .map_err(|e| e.to_string())?;

    let total_bytes = source.len();
    let step = size - overlap;
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut index = 0;

    while start < total_bytes {
        let end = (start + size).min(total_bytes);
        chunks.push(ChunkInfo { index, start, end });
        index += 1;
        start += step;
        if end >= total_bytes {
            break;
        }
    }

    Ok(ChunkIndicesResponse {
        file: file.to_string(),
        total_bytes,
        chunk_size: size,
        overlap,
        chunks,
    })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use super::*;

    #[test]
    fn test_chunk_indices_basic() {
        let file_size = 1000;
        let chunks = calculate_chunks(file_size, 400, 100);
        assert_eq!(chunks, vec![
            (0, 400),
            (300, 700),
            (600, 1000),
        ]);
    }

    #[test]
    fn test_chunk_indices_small_file() {
        let chunks = calculate_chunks(50, 400, 100);
        assert_eq!(chunks, vec![(0, 50)]);
    }

    #[test]
    fn test_chunk_indices_exact_fit() {
        let chunks = calculate_chunks(400, 400, 100);
        assert_eq!(chunks, vec![(0, 400)]);
    }

    #[test]
    fn test_chunk_indices_no_overlap() {
        let chunks = calculate_chunks(1000, 200, 0);
        assert_eq!(chunks, vec![
            (0, 200),
            (200, 400),
            (400, 600),
            (600, 800),
            (800, 1000),
        ]);
    }

    #[test]
    fn test_grep_scope_parse() {
        assert_eq!("all".parse::<GrepScope>().unwrap(), GrepScope::All);
        assert_eq!("code".parse::<GrepScope>().unwrap(), GrepScope::Code);
    }

    #[test]
    fn test_grep_scope_parse_invalid() {
        assert!("unknown".parse::<GrepScope>().is_err());
        assert!("".parse::<GrepScope>().is_err());
    }

    fn calculate_chunks(total_bytes: usize, chunk_size: usize, overlap: usize) -> Vec<(usize, usize)> {
        let step = chunk_size - overlap;
        let mut chunks = Vec::new();
        let mut start = 0;
        while start < total_bytes {
            let end = (start + chunk_size).min(total_bytes);
            chunks.push((start, end));
            start += step;
            if end >= total_bytes {
                break;
            }
        }
        chunks
    }
}
