use pax_engine::api::*;
use pax_engine::*;
use pax_manifest::*;
use pax_std::*;

use crate::controls::settings::AREAS_PROP;

use super::PropertyEditorData;

#[pax]
#[engine_import_path("pax_engine")]
#[file("controls/settings/property_editor/border_radius_property_editor.pax")]
pub struct BorderRadiusPropertyEditor {
    pub data: Property<PropertyEditorData>,

    // All the below props should be private: never set by user, used for internal state
    pub last_definition: Property<String>,
    pub textbox: Property<String>,
    pub error: Property<String>,
}

impl BorderRadiusPropertyEditor {
    pub fn on_mount(&mut self, ctx: &NodeContext) {
        let index = self.data.get().editor_index;
        if index != 0 {
            AREAS_PROP.with(|areas| {
                areas.update(|areas| {
                    while areas.len() <= index {
                        areas.push(0.0)
                    }
                    areas[index - 1] = 40.0;
                });
            });
        }
        let data = self.data.clone();
        let deps = [data.untyped()];
        let ctx = ctx.clone();
        let err = self.error.clone();
        self.textbox.replace_with(Property::computed(
            move || {
                err.set("".to_string());
                data.get().get_value_as_str(&ctx)
            },
            &deps,
        ));
    }

    pub fn text_input(&mut self, _ctx: &NodeContext, args: Event<TextboxInput>) {
        self.textbox.set(args.text.to_owned());
    }

    pub fn text_change(&mut self, ctx: &NodeContext, args: Event<TextboxChange>) {
        self.textbox.set(args.text.to_owned());
        if let Err(_error) = self.data.get().set_value(ctx, &args.text) {
            self.error.set("error".to_owned());
        } else {
            self.error.set("".to_owned());
        }
    }
}
