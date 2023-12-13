#![allow(unused_imports)]

mod slotted;

use pax_lang::api::*;
use pax_lang::*;
use pax_std::components::Stacker;
use pax_std::components::*;
use pax_std::primitives::*;
use pax_std::types::text::*;
use pax_std::types::*;
use slotted::Slotted;

#[derive(Pax)]
#[main]
#[file("lib.pax")]
pub struct Example {}

impl Example {
    pub fn handle_mount(&mut self, ctx: &NodeContext) {}
}