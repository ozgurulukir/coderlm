use super::{LanguageConfig, TestPattern};

/// Symbol extraction queries for Vue Single File Components.
///
/// Vue SFCs parsed by tree-sitter-vue include script content as part of the AST.
/// The queries below cover:
/// - Standard TypeScript patterns (functions, classes, interfaces, etc.)
/// - Vue-specific patterns (defineComponent, defineProps, defineEmits, composables)
/// - <script setup> implicit bindings (const/let at top level become template-accessible)
pub const SYMBOLS_QUERY: &str = r#"
(function_declaration
  name: (identifier) @function.name) @function.def

(class_declaration
  name: (type_identifier) @class.name) @class.def

(method_definition
  name: (property_identifier) @method.name) @method.def

(lexical_declaration
  (variable_declarator
    name: (identifier) @const.name
    value: (arrow_function))) @const.def

(lexical_declaration
  (variable_declarator
    name: (identifier) @const.name
    value: (call_expression
      function: (identifier) @_define_fn
      (#eq? @_define_fn "defineComponent")))) @const.def

(lexical_declaration
  (variable_declarator
    name: (identifier) @const.name
    value: (call_expression
      function: (member_expression
        object: (identifier) @_vue
        property: (property_identifier) @_define)
      (#eq? @_vue "Vue")
      (#match? @_define "^define")))) @const.def

(interface_declaration
  name: (type_identifier) @interface.name) @interface.def

(type_alias_declaration
  name: (type_identifier) @type.name) @type.def

(enum_declaration
  name: (identifier) @enum.name) @enum.def

(call_expression
  function: (identifier) @_macro
  arguments: (arguments
    (object) @macro.args)
  (#match? @_macro "^define"))
"#;

pub const CALLERS_QUERY: &str = r#"
(call_expression
  function: (identifier) @callee)

(call_expression
  function: (member_expression
    property: (property_identifier) @callee))
"#;

pub const VARIABLES_QUERY: &str = r#"
(variable_declarator
  name: (identifier) @var.name)

(variable_declarator
  name: (object_pattern
    (shorthand_property_identifier_pattern) @var.name))

(variable_declarator
  name: (array_pattern
    (identifier) @var.name))

(for_in_statement
  left: (identifier) @var.name)

(for_in_statement
  left: (lexical_declaration
    (variable_declarator
      name: (identifier) @var.name)))

(required_parameter
  pattern: (identifier) @var.name)

(optional_parameter
  pattern: (identifier) @var.name)
"#;

pub fn config() -> LanguageConfig {
    LanguageConfig {
        language: tree_sitter_vue_updated::language(),
        symbols_query: SYMBOLS_QUERY,
        callers_query: CALLERS_QUERY,
        variables_query: VARIABLES_QUERY,
        test_patterns: vec![
            TestPattern::CallExpression("it"),
            TestPattern::CallExpression("test"),
            TestPattern::CallExpression("describe"),
        ],
    }
}
