//! # PaxManifestORM API
//!
//! `PaxManifestORM` provides an interface for managing `PaxManifest` objects, allowing for easy management of template nodes, selectors, and handlers.
//!
//! ## Main Functions
//!
//! - `build_new_node`: Create a new node builder instance. This method initializes a `NodeBuilder` for creating a new template node.
//! - `get_node`: Retrieve an existing node. This method returns a `NodeBuilder` initialized with an existing node's data.
//! - `remove_node`: Remove a specified node from the manifest.
//! - `build_new_selector`: Create a new selector builder instance. This method initializes a `SelectorBuilder` for creating a new selector.
//! - `get_selector`: Retrieve an existing selector. This method returns a `SelectorBuilder` initialized with an existing selector's data.
//! - `remove_selector`: Remove a specified selector from the manifest.
//! - `build_new_handler`: Create a new handler builder instance. This method initializes a `HandlerBuilder` for creating a new handler.
//! - `get_handler`: Retrieve an existing handler. This method returns a `HandlerBuilder` initialized with an existing handler's data.
//! - `remove_handler`: Remove a specified handler from the manifest.
//! - `execute_command`: Execute a command that implements the `Command` trait, allowing for actions like adding, updating, or removing nodes, selectors, and handlers.
//! - `undo`: Undo the last command. This method rolls back the last change made to the manifest.
//! - `redo`: Redo the last undone command. This method reapplies the last change that was undone.
//! - `undo_until`: Undo commands up to a specified command ID. This allows for targeted rollback of multiple changes.
//!
//! For usage examples see the tests in `pax-designtime/src/orm/tests.rs`.

use std::collections::HashMap;

use pax_manifest::pax_runtime_api::{Interpolatable, Property};
use pax_manifest::{
    ComponentDefinition, ComponentTemplate, NodeLocation, PaxManifest, SettingElement,
    TemplateNodeDefinition, TemplateNodeId, TypeId, UniqueTemplateNodeIdentifier, ValueDefinition,
};
use serde_derive::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json;

use self::template::{builder::NodeBuilder, ConvertToComponentRequest, RemoveTemplateNodeRequest};
use self::template::{GetChildrenRequest, MoveTemplateNodeRequest, PasteSubTreeRequest};

use anyhow::{anyhow, Result};
pub mod template;
#[cfg(test)]
mod tests;

pub trait Request {
    type Response: Response;
}

pub trait Response {
    fn set_id(&mut self, id: usize);
    fn get_id(&self) -> usize;
    fn get_reload_type(&self) -> Option<ReloadType> {
        None
    }
}

pub trait Command<R: Request> {
    fn execute(&mut self, manifest: &mut PaxManifest) -> Result<R::Response, String>;
    fn as_undo_redo(&mut self) -> Option<UndoRedoCommand> {
        None
    }
}

#[derive(Serialize, Deserialize)]
pub struct PaxManifestORM {
    manifest: PaxManifest,
    undo_stack: Vec<(usize, UndoRedoCommand)>,
    redo_stack: Vec<(usize, UndoRedoCommand)>,
    next_command_id: usize,
    // This counter increase with each command execution/undo/redo (essentially tracks each unique change to the manifest)
    manifest_version: Property<usize>,
    next_new_component_id: usize,
    new_components: Vec<TypeId>,
    reload_queue: Vec<ReloadType>,
    pub manifest_loaded_from_server: Property<bool>,
}

impl PaxManifestORM {
    pub fn new(manifest: PaxManifest) -> Self {
        PaxManifestORM {
            manifest,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            next_command_id: 0,
            manifest_version: Property::new(0),
            next_new_component_id: 1,
            new_components: Vec::new(),
            reload_queue: Vec::new(),
            manifest_loaded_from_server: Property::new(false),
        }
    }

    pub fn get_new_components(&mut self) -> Vec<ComponentDefinition> {
        let mut new_components_to_process = Vec::new();

        for component_type_id in &self.new_components {
            if let Some(component) = self.manifest.components.get(component_type_id) {
                new_components_to_process.push(component.clone());
            }
        }
        self.new_components.clear();
        new_components_to_process
    }

    pub fn get_manifest(&self) -> &PaxManifest {
        &self.manifest
    }

    pub fn set_manifest(&mut self, manifest: PaxManifest) {
        self.manifest = manifest;
        self.increment_manifest_version();
        self.manifest_loaded_from_server.set(true);
        self.set_reload(ReloadType::FullEdit);
    }

    pub fn get_manifest_version(&self) -> Property<usize> {
        self.manifest_version.clone()
    }

    pub fn set_reload(&mut self, reload_type: ReloadType) {
        self.reload_queue.push(reload_type);
    }

    pub fn set_userland_root_component_type_id(&mut self, type_id: &TypeId) {
        self.manifest.main_component_type_id = type_id.clone();
    }

    pub fn take_reload_queue(&mut self) -> Vec<ReloadType> {
        std::mem::take(&mut self.reload_queue)
    }

    pub fn increment_manifest_version(&mut self) {
        self.manifest_version.update(|v| *v += 1);
    }

    pub fn build_new_node(
        &mut self,
        containing_component_type_id: TypeId,
        node_type_id: TypeId,
    ) -> NodeBuilder {
        NodeBuilder::new(self, containing_component_type_id, node_type_id, true)
    }

    pub fn get_node_location(&self, uni: &UniqueTemplateNodeIdentifier) -> Option<NodeLocation> {
        let component = self
            .manifest
            .components
            .get(&uni.get_containing_component_type_id())?;
        let template = component.template.as_ref()?;
        template.get_location(&uni.get_template_node_id())
    }

    pub fn get_siblings(
        &self,
        uni: &UniqueTemplateNodeIdentifier,
    ) -> Option<Vec<UniqueTemplateNodeIdentifier>> {
        let component = self
            .manifest
            .components
            .get(&uni.get_containing_component_type_id())?;
        let template = component.template.as_ref()?;
        Some(
            template
                .get_siblings(&uni.get_template_node_id())?
                .into_iter()
                .map(|tid| {
                    UniqueTemplateNodeIdentifier::build(uni.get_containing_component_type_id(), tid)
                })
                .collect(),
        )
    }

    pub fn get_parent(
        &self,
        uni: &UniqueTemplateNodeIdentifier,
    ) -> Option<UniqueTemplateNodeIdentifier> {
        let component = self
            .manifest
            .components
            .get(&uni.get_containing_component_type_id())?;
        let template = component.template.as_ref()?;
        Some(UniqueTemplateNodeIdentifier::build(
            uni.get_containing_component_type_id(),
            template.get_parent(&uni.get_template_node_id())?,
        ))
    }

    pub fn move_node(
        &mut self,
        uni: UniqueTemplateNodeIdentifier,
        location: NodeLocation,
    ) -> Result<usize, String> {
        let res = self.execute_command(MoveTemplateNodeRequest::new(uni, location))?;
        Ok(res.get_id())
    }

    pub fn get_node_children(
        &mut self,
        uni: UniqueTemplateNodeIdentifier,
    ) -> Result<Vec<UniqueTemplateNodeIdentifier>, String> {
        let resp = self
            .execute_command(GetChildrenRequest { uni: uni.clone() })
            .unwrap();
        Ok(resp.children)
    }

    pub fn swap_main_component(&mut self, component: ComponentDefinition) -> Result<(), String> {
        let command = template::SwapMainComponentRequest::new(component);
        self.execute_command(command)?;
        Ok(())
    }

    pub fn copy_subtrees(&self, type_id: &TypeId, nodes: &[TemplateNodeId]) -> Option<SubTrees> {
        let roots: Vec<_> = nodes.iter().cloned().collect();
        let mut children = HashMap::new();
        let mut nodes = HashMap::new();

        let component = self.manifest.components.get(type_id)?;
        let template = component.template.as_ref()?;
        let mut to_visit: Vec<_> = roots.iter().cloned().collect();
        while let Some(node) = to_visit.pop() {
            if let Some(node_def) = template.get_node(&node) {
                nodes.insert(node.clone(), node_def.clone());
                let node_children = template.get_children(&node).unwrap_or_default();
                children.insert(node.clone(), node_children.clone());
                to_visit.extend(node_children);
            }
        }

        Some(SubTrees {
            roots,
            children,
            nodes,
        })
    }

    pub fn paste_subtrees(
        &mut self,
        location: NodeLocation,
        subtrees: SubTrees,
    ) -> Result<Vec<TemplateNodeId>, String> {
        let res = self.execute_command(PasteSubTreeRequest::new(location, subtrees))?;
        Ok(res.get_created().to_vec())
    }

    pub fn get_node(
        &mut self,
        uni: UniqueTemplateNodeIdentifier,
        overwrite_expressions: bool,
    ) -> Option<NodeBuilder> {
        NodeBuilder::retrieve_node(self, uni, overwrite_expressions)
    }

    pub fn get_main_component(&self) -> &TypeId {
        &self.manifest.main_component_type_id
    }

    pub fn get_components(&self) -> Vec<TypeId> {
        self.manifest.components.keys().cloned().collect()
    }

    pub fn get_component(&self, type_id: &TypeId) -> anyhow::Result<&ComponentDefinition> {
        self.manifest
            .components
            .get(type_id)
            .ok_or(anyhow!("couldn't find component"))
    }

    pub fn get_property(
        &self,
        unid: &UniqueTemplateNodeIdentifier,
        key: &str,
    ) -> Option<ValueDefinition> {
        let tnd = self.manifest.get_template_node(unid)?;
        if let Some(settings) = &tnd.settings {
            for setting in settings {
                if let SettingElement::Setting(token, value) = setting {
                    if token.token_value == key {
                        return Some(value.clone());
                    }
                }
            }
        }
        None
    }

    pub fn get_property_type(
        &self,
        unid: &UniqueTemplateNodeIdentifier,
        key: &str,
    ) -> Option<TypeId> {
        let tnd = self.manifest.get_template_node(unid)?;
        let property_types = self.manifest.type_table.get(&tnd.type_id)?;
        property_types
            .property_definitions
            .iter()
            .find(|v| v.name == key)
            .map(|v| v.type_id.clone())
    }

    pub fn remove_node(&mut self, uni: UniqueTemplateNodeIdentifier) -> Result<usize, String> {
        let command = RemoveTemplateNodeRequest::new(uni);
        let resp = self.execute_command(command)?;
        Ok(resp.get_id())
    }

    pub fn move_to_new_component(
        &mut self,
        nodes: &[MoveToComponentEntry],
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> Result<(), String> {
        let new_component_number = self.next_new_component_id;
        let command = ConvertToComponentRequest::new(
            nodes.to_vec(),
            new_component_number,
            x,
            y,
            width,
            height,
        );
        let resp = self.execute_command(command)?;
        self.new_components.push(resp.new_component_type_id);
        self.next_new_component_id += 1;
        Ok(())
    }

    pub fn execute_command<R: Request, C>(&mut self, mut command: C) -> Result<R::Response, String>
    where
        C: Command<R>,
    {
        let mut response: <R as Request>::Response = command.execute(&mut self.manifest)?;
        let command_id = self.next_command_id;
        if let Some(command) = command.as_undo_redo() {
            self.undo_stack.push((command_id, command));
            self.redo_stack.clear();
        }

        response.set_id(command_id);
        self.next_command_id += 1;
        if let Some(reload_type) = response.get_reload_type() {
            self.set_reload(reload_type);
            self.manifest_version.update(|v| *v += 1);
        }

        Ok(response)
    }

    pub fn undo(&mut self) -> Result<(), String> {
        if let Some((id, mut command)) = self.undo_stack.pop() {
            command.undo(&mut self.manifest)?;
            self.redo_stack.push((id, command));
            self.manifest_version.update(|v| *v += 1);
            self.set_reload(ReloadType::FullEdit);
        }
        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), String> {
        if let Some((id, mut command)) = self.redo_stack.pop() {
            command.redo(&mut self.manifest)?;
            self.undo_stack.push((id, command));
            self.manifest_version.update(|v| *v += 1);
            self.set_reload(ReloadType::FullEdit);
        }
        Ok(())
    }

    pub fn get_last_undo_id(&self) -> Option<usize> {
        self.undo_stack.last().map(|l| l.0)
    }

    pub fn undo_until(&mut self, command_id: Option<usize>) -> Result<(), String> {
        while let Some((id, _)) = self.undo_stack.last() {
            if command_id.is_some_and(|c_id| c_id == *id) {
                break;
            }
            self.undo()?;
        }
        Ok(())
    }

    pub fn redo_including(&mut self, command_id: usize) -> Result<(), String> {
        while let Some(&(id, _)) = self.redo_stack.last() {
            self.redo()?;
            if id == command_id {
                break;
            }
        }
        Ok(())
    }

    pub fn replace_template(
        &mut self,
        component_type_id: TypeId,
        template: ComponentTemplate,
    ) -> Result<usize, String> {
        let command = template::ReplaceTemplateRequest::new(component_type_id, template);
        let resp = self.execute_command(command)?;
        Ok(resp.get_id())
    }

    pub fn component_has_slots(&self, type_id: &TypeId) -> bool {
        let Some(component) = self.manifest.components.get(type_id) else {
            return false;
        };
        let Some(template) = component.template.as_ref() else {
            return false;
        };
        template.contains_slots()
    }
}

pub trait Undo {
    fn undo(&mut self, manifest: &mut PaxManifest) -> Result<(), String>;
}

#[derive(Serialize, Deserialize)]
pub enum UndoRedoCommand {
    AddTemplateNodeRequest(Box<template::AddTemplateNodeRequest>),
    RemoveTemplateNodeRequest(Box<template::RemoveTemplateNodeRequest>),
    MoveTemplateNodeRequest(Box<template::MoveTemplateNodeRequest>),
    UpdateTemplateNodeRequest(Box<template::UpdateTemplateNodeRequest>),
    PasteSubTreeRequest(Box<template::PasteSubTreeRequest>),
    ReplaceTemplateRequest(Box<template::ReplaceTemplateRequest>),
    ConvertToComponentRequest(Box<template::ConvertToComponentRequest>),
    SwapMainComponentRequest(Box<template::SwapMainComponentRequest>),
}

impl UndoRedoCommand {
    fn undo(&mut self, manifest: &mut PaxManifest) -> Result<(), String> {
        match self {
            UndoRedoCommand::MoveTemplateNodeRequest(command) => command.undo(manifest),
            UndoRedoCommand::AddTemplateNodeRequest(command) => command.undo(manifest),
            UndoRedoCommand::RemoveTemplateNodeRequest(command) => command.undo(manifest),
            UndoRedoCommand::UpdateTemplateNodeRequest(command) => command.undo(manifest),
            UndoRedoCommand::PasteSubTreeRequest(command) => command.undo(manifest),
            UndoRedoCommand::ReplaceTemplateRequest(command) => command.undo(manifest),
            UndoRedoCommand::ConvertToComponentRequest(command) => command.undo(manifest),
            UndoRedoCommand::SwapMainComponentRequest(command) => command.undo(manifest),
        }
    }

    fn redo(&mut self, manifest: &mut PaxManifest) -> Result<(), String> {
        match self {
            UndoRedoCommand::AddTemplateNodeRequest(command) => {
                let _ = command.execute(manifest);
            }
            UndoRedoCommand::RemoveTemplateNodeRequest(command) => {
                let _ = command.execute(manifest);
            }
            UndoRedoCommand::MoveTemplateNodeRequest(command) => {
                let _ = command.execute(manifest);
            }
            UndoRedoCommand::UpdateTemplateNodeRequest(command) => {
                let _ = command.execute(manifest);
            }
            UndoRedoCommand::PasteSubTreeRequest(command) => {
                let _ = command.execute(manifest);
            }
            UndoRedoCommand::ReplaceTemplateRequest(command) => {
                let _ = command.execute(manifest);
            }
            UndoRedoCommand::ConvertToComponentRequest(command) => {
                let _ = command.execute(manifest);
            }
            UndoRedoCommand::SwapMainComponentRequest(command) => {
                let _ = command.execute(manifest);
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MoveToComponentEntry {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub id: UniqueTemplateNodeIdentifier,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ReloadType {
    FullEdit,
    Partial(UniqueTemplateNodeIdentifier),
    FullPlay,
}

impl Interpolatable for SubTrees {}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SubTrees {
    roots: Vec<TemplateNodeId>,
    children: HashMap<TemplateNodeId, Vec<TemplateNodeId>>,
    nodes: HashMap<TemplateNodeId, TemplateNodeDefinition>,
}
