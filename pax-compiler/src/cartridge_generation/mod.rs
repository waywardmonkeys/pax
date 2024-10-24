//! # Code Generation Module
//!
//! The `code_generation` module provides structures and functions for generating Pax Cartridges
//! from Pax Manifests. The `generate_and_overwrite_cartridge` function is the main entrypoint.

use std::fs;

use pax_manifest::{cartridge_generation::CommonProperty, PaxManifest};

use std::path::PathBuf;

pub mod templating;

pub const CARTRIDGE_PARTIAL_PATH: &str = "cartridge.partial.rs";

// Generates (codegens) the PaxCartridge definition, abiding by the PaxCartridge trait.
// Side-effect: writes the generated string to disk as .pax/cartridge.partial.rs,
// so that it may be `include!`d by the  #[pax] #[main] macro
pub fn generate_cartridge_partial_rs(
    pax_dir: &PathBuf,
    merged_manifest: &PaxManifest,
    userland_manifest: &PaxManifest,
    designer_manifest: Option<PaxManifest>,
) -> PathBuf {
    //press template into String
    let generated_lib_rs = templating::press_template_codegen_cartridge_snippet(
        templating::TemplateArgsCodegenCartridgeSnippet {
            cartridge_struct_id: merged_manifest.get_main_cartridge_struct_id(),
            definition_to_instance_traverser_struct_id: merged_manifest
                .get_main_definition_to_instance_traverser_struct_id(),
            components: merged_manifest.generate_codegen_component_info(),
            common_properties: CommonProperty::get_as_common_property(),
            type_table: merged_manifest.type_table.clone(),
            is_designtime: cfg!(feature = "designtime"),
            userland_manifest_json: serde_json::to_string(userland_manifest).unwrap(),
            designer_manifest_json: if let Some(designer_manifest) = designer_manifest {
                serde_json::to_string(&designer_manifest).unwrap()
            } else {
                "{}".to_string()
            },
            engine_import_path: userland_manifest.engine_import_path.clone(),
        },
    );

    let path = pax_dir.join(CARTRIDGE_PARTIAL_PATH);
    fs::write(path.clone(), generated_lib_rs).unwrap();
    path
}
