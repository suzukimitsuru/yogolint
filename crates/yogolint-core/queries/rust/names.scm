; yogolint: definitions that the bundled tags.scm of tree-sitter-rust does not capture.
; Uses the same capture names as tags.scm (@name + @definition.<kind>).

; trait method signatures (tags.scm only captures function_item)
(function_signature_item
    name: (identifier) @name) @definition.method

; variables
(let_declaration
    pattern: (identifier) @name) @definition.variable
(let_declaration
    pattern: (mut_pattern (identifier) @name)) @definition.variable
(let_declaration
    pattern: (tuple_pattern (identifier) @name)) @definition.variable
(let_condition
    pattern: (identifier) @name) @definition.variable
(for_expression
    pattern: (identifier) @name) @definition.variable
(for_expression
    pattern: (tuple_pattern (identifier) @name)) @definition.variable

; parameters
(parameter
    pattern: (identifier) @name) @definition.parameter
(parameter
    pattern: (mut_pattern (identifier) @name)) @definition.parameter
(closure_parameters
    (identifier) @name) @definition.parameter

; fields
(field_declaration
    name: (field_identifier) @name) @definition.field

; constants
(const_item
    name: (identifier) @name) @definition.constant
(static_item
    name: (identifier) @name) @definition.constant
(enum_variant
    name: (identifier) @name) @definition.constant
