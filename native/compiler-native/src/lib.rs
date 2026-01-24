use napi_derive::napi;

mod codegen;
mod component;
mod discovery;
mod expression_tests;
mod finalize;
mod jsx_lowerer;
mod layout;
mod lowering_tests;
mod parse;
mod transform;
mod validate;

pub use codegen::generate_codegen_intent;
pub use component::resolve_components_native;
pub use discovery::{
    discover_components_native, discover_layouts_native, extract_page_bindings_native,
    extract_styles_native,
};
pub use finalize::finalize_output_native;
pub use layout::process_layout_native;
pub use parse::{is_component_tag_native, parse_script_native, parse_template_native};
pub use transform::{
    analyze_expressions, classify_expression_native, evaluate_expression_native,
    lower_fragments_native, transform_template_native,
};
pub use validate::*;

#[napi]
pub fn compile_bridge() -> String {
    "Zenith Native Bridge Connected".to_string()
}
