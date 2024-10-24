//! # The Pax Compiler Library
//!
//! `pax-compiler` is a collection of utilities to facilitate compiling Pax templates into Rust code.
//!
//! This library is structured into several modules, each providing different
//! functionality:
//!
//! - `building`: Core structures and functions related to building management.
//! - `utilities`: Helper functions and common routines used across the library.
//!

#[macro_use]
extern crate serde;

extern crate core;
mod building;
mod cartridge_generation;
pub mod formatting;
pub mod helpers;

pub mod design_server;

use color_eyre::eyre;
use color_eyre::eyre::Report;
use eyre::eyre;
use fs_extra::dir::{self, CopyOptions};
use helpers::{copy_dir_recursively, wait_with_output, ERR_SPAWN};
use pax_manifest::{
    ComponentDefinition, ComponentTemplate, PaxManifest, TemplateNodeDefinition, TypeId,
};
use std::fs;
use std::io::Write;
use std::sync::{Arc, Mutex};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use crate::building::build_project_with_cartridge;

use crate::cartridge_generation::generate_cartridge_partial_rs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::helpers::{
    get_or_create_pax_directory, update_pax_dependency_versions, INTERFACE_DIR_NAME, PAX_BADGE,
    PAX_CREATE_LIBDEV_TEMPLATE_DIR_NAME, PAX_CREATE_TEMPLATE, PAX_IOS_INTERFACE_TEMPLATE,
    PAX_MACOS_INTERFACE_TEMPLATE, PAX_SWIFT_CARTRIDGE_TEMPLATE, PAX_SWIFT_COMMON_TEMPLATE,
    PAX_WEB_INTERFACE_TEMPLATE,
};

pub struct RunContext {
    pub target: RunTarget,
    pub project_path: PathBuf,
    pub verbose: bool,
    pub should_also_run: bool,
    pub is_libdev_mode: bool,
    pub process_child_ids: Arc<Mutex<Vec<u64>>>,
    pub should_run_designer: bool,
    pub is_release: bool,
}

#[derive(PartialEq)]
pub enum RunTarget {
    #[allow(non_camel_case_types)]
    macOS,
    Web,
    #[allow(non_camel_case_types)]
    iOS,
}

/// For the specified file path or current working directory, first compile Pax project,
/// then run it with a patched build of the `chassis` appropriate for the specified platform
/// See: pax-compiler-sequence-diagram.png
pub fn perform_build(ctx: &RunContext) -> eyre::Result<(PaxManifest, Option<PathBuf>), Report> {
    //Compile ts files if applicable (this needs to happen before copying to .pax)
    if ctx.is_libdev_mode && ctx.target == RunTarget::Web {
        if let Ok(root) = std::env::var("PAX_WORKSPACE_ROOT") {
            let mut cmd = Command::new("bash");
            cmd.arg("./build-interface.sh");
            let web_interface_path = Path::new(&root)
                .join("pax-compiler")
                .join("files")
                .join("interfaces")
                .join("web");
            cmd.current_dir(&web_interface_path);
            if !cmd
                .output()
                .expect("failed to start process")
                .status
                .success()
            {
                panic!(
                    "failed to build js files running ./build-interface.sh at {:?}",
                    web_interface_path
                );
            };
        } else {
            panic!(
                "FATAL: PAX_WORKSPACE_ROOT env variable not set - didn't compile typescript files"
            );
        }
    }

    let pax_dir = get_or_create_pax_directory(&ctx.project_path);

    // Copy interface files for relevant path
    copy_interface_files_for_target(ctx, &pax_dir);

    println!("{} 🛠️  Building parser binary with `cargo`...", *PAX_BADGE);

    // Run parser bin from host project with `--features parser`
    let output = run_parser_binary(
        &ctx.project_path,
        Arc::clone(&ctx.process_child_ids),
        ctx.should_run_designer,
    );

    // Forward stderr only
    std::io::stderr()
        .write_all(output.stderr.as_slice())
        .unwrap();

    if !output.status.success() {
        return Err(eyre!(
            "Parsing failed — there is likely a syntax error in the provided pax"
        ));
    }

    let out = String::from_utf8(output.stdout).unwrap();

    let mut manifests: Vec<PaxManifest> =
        serde_json::from_str(&out).expect(&format!("Malformed JSON from parser: {}", &out));

    // Simple starting convention: first manifest is userland, second manifest is designer; other schemas are undefined
    let mut userland_manifest = manifests.remove(0);

    let mut merged_manifest = userland_manifest.clone();

    //Hack: add a wrapper component so UniqueTemplateNodeIdentifier is a suitable uniqueid, even for root nodes
    let wrapper_type_id = TypeId::build_singleton("ROOT_COMPONENT", Some("RootComponent"));
    let mut tnd = TemplateNodeDefinition::default();
    tnd.type_id = userland_manifest.main_component_type_id.clone();
    let mut wrapper_component_template = ComponentTemplate::new(wrapper_type_id.clone(), None);
    wrapper_component_template.add(tnd);
    userland_manifest.components.insert(
        wrapper_type_id.clone(),
        ComponentDefinition {
            type_id: wrapper_type_id.clone(),
            is_main_component: false,
            is_primitive: false,
            is_struct_only_component: false,
            module_path: "".to_string(),
            primitive_instance_import_path: None,
            template: Some(wrapper_component_template),
            settings: None,
        },
    );

    let designer_manifest = if ctx.should_run_designer {
        let designer_manifest = manifests.remove(0);
        merged_manifest.merge_in_place(&designer_manifest);

        userland_manifest
            .components
            .extend(designer_manifest.components.clone());
        userland_manifest
            .type_table
            .extend(designer_manifest.type_table.clone());

        Some(designer_manifest)
    } else {
        None
    };

    println!("{} 🦀 Generating Rust", *PAX_BADGE);
    generate_cartridge_partial_rs(
        &pax_dir,
        &merged_manifest,
        &userland_manifest,
        designer_manifest,
    );
    // source_map.extract_ranges_from_generated_code(cartridge_path.to_str().unwrap());

    //7. Build full project from source
    println!("{} 🧱 Building project with `cargo`", *PAX_BADGE);
    let build_dir = build_project_with_cartridge(
        &pax_dir,
        &ctx,
        Arc::clone(&ctx.process_child_ids),
        merged_manifest.assets_dirs,
        userland_manifest.clone(),
    )?;

    Ok((userland_manifest, build_dir))
}

fn copy_interface_files_for_target(ctx: &RunContext, pax_dir: &PathBuf) {
    let target_str: &str = (&ctx.target).into();
    let target_str_lower = &target_str.to_lowercase();
    let interface_path = pax_dir.join(INTERFACE_DIR_NAME).join(target_str_lower);

    let _ = fs::remove_dir_all(&interface_path);
    let _ = fs::create_dir_all(&interface_path);

    let mut custom_interface = pax_dir
        .parent()
        .unwrap()
        .join("interfaces")
        .join(target_str_lower);
    if ctx.target == RunTarget::Web {
        custom_interface = custom_interface.join("public");
    }

    if custom_interface.exists() {
        copy_interface_files(&custom_interface, &interface_path);
    } else {
        copy_default_interface_files(&interface_path, ctx);
    }

    // Copy common files for macOS and iOS builds
    if matches!(ctx.target, RunTarget::macOS | RunTarget::iOS) {
        let common_dest = pax_dir.join(INTERFACE_DIR_NAME).join("common");
        copy_common_swift_files(ctx, &common_dest);
    }
}

fn copy_interface_files(src: &Path, dest: &Path) {
    copy_dir_recursively(src, dest, &[]).expect("Failed to copy interface files");
}

fn copy_default_interface_files(interface_path: &Path, ctx: &RunContext) {
    if ctx.is_libdev_mode {
        let pax_compiler_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let interface_src = match ctx.target {
            RunTarget::Web => pax_compiler_root
                .join("files")
                .join("interfaces")
                .join("web")
                .join("public"),
            RunTarget::macOS => pax_compiler_root
                .join("files")
                .join("interfaces")
                .join("macos")
                .join("pax-app-macos"),
            RunTarget::iOS => pax_compiler_root
                .join("files")
                .join("interfaces")
                .join("ios")
                .join("pax-app-ios"),
        };

        copy_dir_recursively(&interface_src, interface_path, &[])
            .expect("Failed to copy interface files");
    } else {
        // File src is include_dir — recursively extract files from include_dir into full_path
        match ctx.target {
            RunTarget::Web => PAX_WEB_INTERFACE_TEMPLATE
                .extract(interface_path)
                .expect("Failed to extract web interface files"),
            RunTarget::macOS => PAX_MACOS_INTERFACE_TEMPLATE
                .extract(interface_path)
                .expect("Failed to extract macos interface files"),
            RunTarget::iOS => PAX_IOS_INTERFACE_TEMPLATE
                .extract(interface_path)
                .expect("Failed to extract ios interface files"),
        }
    }
}

fn copy_common_swift_files(ctx: &RunContext, common_dest: &Path) {
    if ctx.is_libdev_mode {
        let pax_compiler_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let common_swift_cartridge_src = pax_compiler_root
            .join("files")
            .join("swift")
            .join("pax-swift-cartridge");
        let common_swift_common_src = pax_compiler_root
            .join("files")
            .join("swift")
            .join("pax-swift-common");
        let common_swift_cartridge_dest = common_dest.join("pax-swift-cartridge");
        let common_swift_common_dest = common_dest.join("pax-swift-common");

        copy_dir_recursively(
            &common_swift_cartridge_src,
            &common_swift_cartridge_dest,
            &[],
        )
        .expect("Failed to copy swift cartridge files");
        copy_dir_recursively(&common_swift_common_src, &common_swift_common_dest, &[])
            .expect("Failed to copy swift common files");
    } else {
        PAX_SWIFT_COMMON_TEMPLATE
            .extract(common_dest)
            .expect("Failed to extract swift common template files");
        PAX_SWIFT_CARTRIDGE_TEMPLATE
            .extract(common_dest)
            .expect("Failed to extract swift cartridge template files");
    }
}

/// Ejects the interface files for the specified target platform
/// Interface files will then be used to build the project
pub fn perform_eject(ctx: &RunContext) -> eyre::Result<(), Report> {
    let pax_dir = get_or_create_pax_directory(&ctx.project_path);
    eject_interface_files(ctx, &pax_dir);
    Ok(())
}

fn eject_interface_files(ctx: &RunContext, pax_dir: &PathBuf) {
    let target_str: &str = (&ctx.target).into();
    let target_str_lower = &target_str.to_lowercase();
    let custom_interfaces_dir = pax_dir.parent().unwrap().join("interfaces");
    let mut target_custom_interface_dir = custom_interfaces_dir.join(target_str_lower);
    if ctx.target == RunTarget::Web {
        target_custom_interface_dir = target_custom_interface_dir.join("public");
    }

    let _ = fs::create_dir_all(&target_custom_interface_dir);

    if ctx.is_libdev_mode {
        let src_path = get_libdev_interface_path(ctx);
        let _ = copy_dir_recursively(&src_path, &target_custom_interface_dir, &[]);
    } else {
        let _ = extract_interface_template(ctx, &target_custom_interface_dir);
    }

    println!(
        "Interface files ejected to: {}",
        target_custom_interface_dir.display()
    );
}

fn get_libdev_interface_path(ctx: &RunContext) -> PathBuf {
    let pax_compiler_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    match ctx.target {
        RunTarget::Web => pax_compiler_root
            .join("files")
            .join("interfaces")
            .join("web")
            .join("public"),
        RunTarget::macOS => pax_compiler_root
            .join("files")
            .join("interfaces")
            .join("macos")
            .join("pax-app-macos"),
        RunTarget::iOS => pax_compiler_root
            .join("files")
            .join("interfaces")
            .join("ios")
            .join("pax-app-ios"),
    }
}

fn extract_interface_template(ctx: &RunContext, dest: &Path) -> Result<(), std::io::Error> {
    match ctx.target {
        RunTarget::Web => PAX_WEB_INTERFACE_TEMPLATE.extract(dest)?,
        RunTarget::macOS => PAX_MACOS_INTERFACE_TEMPLATE.extract(dest)?,
        RunTarget::iOS => PAX_IOS_INTERFACE_TEMPLATE.extract(dest)?,
    }
    Ok(())
}

/// Clean all `.pax` temp files
pub fn perform_clean(path: &str) {
    let path = PathBuf::from(path);
    let pax_dir = path.join(".pax");
    fs::remove_dir_all(&pax_dir).ok();
}

pub struct CreateContext {
    pub path: String,
    pub is_libdev_mode: bool,
    pub version: String,
}

pub fn perform_create(ctx: &CreateContext) {
    let full_path = Path::new(&ctx.path);

    // Abort if directory already exists
    if full_path.exists() {
        panic!("Error: destination `{:?}` already exists", full_path);
    }
    let _ = fs::create_dir_all(&full_path);

    // clone template into full_path
    if ctx.is_libdev_mode {
        //For is_libdev_mode, we copy our monorepo @/pax-compiler/new-project-template directory
        //to the target directly.  This enables iterating on new-project-template during libdev
        //without the sticky caches associated with `include_dir`
        let pax_compiler_cargo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let template_src = pax_compiler_cargo_root
            .join("files")
            .join("new-project")
            .join(PAX_CREATE_LIBDEV_TEMPLATE_DIR_NAME);

        let mut options = CopyOptions::new();
        options.overwrite = true;

        for entry in std::fs::read_dir(&template_src).expect("Failed to read template directory") {
            let entry_path = entry.expect("Failed to read entry").path();
            if entry_path.is_dir() {
                dir::copy(&entry_path, &full_path, &options).expect("Failed to copy directory");
            } else {
                fs::copy(&entry_path, full_path.join(entry_path.file_name().unwrap()))
                    .expect("Failed to copy file");
            }
        }
    } else {
        // File src is include_dir — recursively extract files from include_dir into full_path
        PAX_CREATE_TEMPLATE
            .extract(&full_path)
            .expect("Failed to extract files");
    }

    //Patch Cargo.toml
    let cargo_template_path = full_path.join("Cargo.toml.template");
    let extracted_cargo_toml_path = full_path.join("Cargo.toml");
    let _ = fs::copy(&cargo_template_path, &extracted_cargo_toml_path);
    let _ = fs::remove_file(&cargo_template_path);

    let crate_name = full_path.file_name().unwrap().to_str().unwrap().to_string();

    // Read the Cargo.toml
    let mut doc = fs::read_to_string(&full_path.join("Cargo.toml"))
        .expect("Failed to read Cargo.toml")
        .parse::<toml_edit::Document>()
        .expect("Failed to parse Cargo.toml");

    // Update the `dependencies` section
    update_pax_dependency_versions(&mut doc, &ctx.version);

    // Update the `package` section
    if let Some(package) = doc
        .as_table_mut()
        .entry("package")
        .or_insert_with(toml_edit::table)
        .as_table_mut()
    {
        if let Some(name_item) = package.get_mut("name") {
            *name_item = toml_edit::Item::Value(crate_name.into());
        }
        if let Some(version_item) = package.get_mut("version") {
            *version_item = toml_edit::Item::Value(ctx.version.clone().into());
        }
    }

    // Write the modified Cargo.toml back to disk
    fs::write(&full_path.join("Cargo.toml"), doc.to_string())
        .expect("Failed to write modified Cargo.toml");

    println!(
        "\nCreated new Pax project at {}.\nTo run:\n  `cd {} && pax-cli run --target=web`",
        full_path.to_str().unwrap(),
        full_path.to_str().unwrap()
    );
}

/// Executes a shell command to run the feature-flagged parser at the specified path
/// Returns an output object containing bytestreams of stdout/stderr as well as an exit code
pub fn run_parser_binary(
    project_path: &PathBuf,
    process_child_ids: Arc<Mutex<Vec<u64>>>,
    should_run_designer: bool,
) -> Output {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(project_path)
        .arg("run")
        .arg("--bin")
        .arg("parser")
        .arg("--features")
        .arg("parser")
        .arg("--profile")
        .arg("parser")
        .arg("--color")
        .arg("always")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    if should_run_designer {
        cmd.arg("--features").arg("designer");
    }

    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(pre_exec_hook);
    }

    let child = cmd.spawn().expect(ERR_SPAWN);

    // child.stdin.take().map(drop);
    let output = wait_with_output(&process_child_ids, child);
    output
}

impl From<&str> for RunTarget {
    fn from(input: &str) -> Self {
        match input.to_lowercase().as_str() {
            "macos" => RunTarget::macOS,
            "web" => RunTarget::Web,
            "ios" => RunTarget::iOS,
            _ => {
                unreachable!()
            }
        }
    }
}

impl<'a> Into<&'a str> for &'a RunTarget {
    fn into(self) -> &'a str {
        match self {
            RunTarget::Web => "Web",
            RunTarget::macOS => "macOS",
            RunTarget::iOS => "iOS",
        }
    }
}

#[cfg(unix)]
fn pre_exec_hook() -> Result<(), std::io::Error> {
    // Set a new process group for this command
    unsafe {
        libc::setpgid(0, 0);
    }
    Ok(())
}
