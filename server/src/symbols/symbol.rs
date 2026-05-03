use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Trait,
    Interface,
    Constant,
    Variable,
    Type,
    Module,
    Import,
    Other,
}

impl std::str::FromStr for SymbolKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "function" | "fn" | "func" => Ok(SymbolKind::Function),
            "method" => Ok(SymbolKind::Method),
            "class" => Ok(SymbolKind::Class),
            "struct" => Ok(SymbolKind::Struct),
            "enum" => Ok(SymbolKind::Enum),
            "trait" => Ok(SymbolKind::Trait),
            "interface" => Ok(SymbolKind::Interface),
            "constant" | "const" => Ok(SymbolKind::Constant),
            "variable" | "var" | "let" => Ok(SymbolKind::Variable),
            "type" => Ok(SymbolKind::Type),
            "module" | "mod" => Ok(SymbolKind::Module),
            "import" | "use" => Ok(SymbolKind::Import),
            _ => Err(format!("Unknown symbol kind: '{}'", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use super::*;

    #[test]
    fn test_symbol_kind_parse_function() {
        assert_eq!("function".parse::<SymbolKind>().unwrap(), SymbolKind::Function);
        assert_eq!("fn".parse::<SymbolKind>().unwrap(), SymbolKind::Function);
        assert_eq!("func".parse::<SymbolKind>().unwrap(), SymbolKind::Function);
    }

    #[test]
    fn test_symbol_kind_parse_class() {
        assert_eq!("class".parse::<SymbolKind>().unwrap(), SymbolKind::Class);
        assert_eq!("struct".parse::<SymbolKind>().unwrap(), SymbolKind::Struct);
        assert_eq!("enum".parse::<SymbolKind>().unwrap(), SymbolKind::Enum);
        assert_eq!("trait".parse::<SymbolKind>().unwrap(), SymbolKind::Trait);
        assert_eq!("interface".parse::<SymbolKind>().unwrap(), SymbolKind::Interface);
    }

    #[test]
    fn test_symbol_kind_parse_other() {
        assert_eq!("module".parse::<SymbolKind>().unwrap(), SymbolKind::Module);
        assert_eq!("mod".parse::<SymbolKind>().unwrap(), SymbolKind::Module);
        assert_eq!("constant".parse::<SymbolKind>().unwrap(), SymbolKind::Constant);
        assert_eq!("const".parse::<SymbolKind>().unwrap(), SymbolKind::Constant);
        assert_eq!("variable".parse::<SymbolKind>().unwrap(), SymbolKind::Variable);
    }

    #[test]
    fn test_symbol_kind_parse_invalid() {
        assert!("foobar".parse::<SymbolKind>().is_err());
        assert!("".parse::<SymbolKind>().is_err());
    }

    #[test]
    fn test_symbol_kind_fromstr_trait() {
        assert_eq!(SymbolKind::from_str("fn").unwrap(), SymbolKind::Function);
        assert!(SymbolKind::from_str("invalid").is_err());
    }

    #[test]
    fn test_symbol_struct_fields() {
        let sym = Symbol {
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            file: "lib.rs".to_string(),
            byte_range: (0, 10),
            line_range: (1, 3),
            language: crate::index::file_entry::Language::Rust,
            signature: "fn foo()".to_string(),
            definition: None,
            parent: None,
        };
        assert_eq!(sym.name, "foo");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert_eq!(sym.file, "lib.rs");
        assert_eq!(sym.byte_range, (0, 10));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file: String,
    pub byte_range: (usize, usize),
    pub line_range: (usize, usize),
    pub language: crate::index::file_entry::Language,
    /// First line of the symbol (e.g. function signature).
    pub signature: String,
    /// Agent-set human-readable description.
    pub definition: Option<String>,
    /// Parent symbol name (e.g. struct for a method).
    pub parent: Option<String>,
}
