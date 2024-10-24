use pax_message::{AnyCreatePatch, NativeInterrupt, SliderPatch};
use pax_runtime::api::{use_RefCell, Layer, Property};
use pax_runtime::{
    BaseInstance, ExpandedNode, InstanceFlags, InstanceNode, InstantiationArgs, RuntimeContext,
};

use_RefCell!();
use pax_runtime::api::*;

use pax_engine::pax;
use std::rc::Rc;

use crate::common::patch_if_needed;

/// A platform-native Slider control
#[pax]
#[engine_import_path("pax_engine")]
#[primitive("pax_std::forms::slider::SliderInstance")]
#[custom(Default)]
pub struct Slider {
    pub background: Property<Color>,
    pub accent: Property<Color>,
    pub border_radius: Property<f64>,
    pub value: Property<f64>,
    pub step: Property<f64>,
    pub min: Property<f64>,
    pub max: Property<f64>,
}

impl Default for Slider {
    fn default() -> Self {
        Self {
            value: Property::new(0.0),
            step: Property::new(1.0),
            min: Property::new(0.0),
            max: Property::new(100.0),
            accent: Property::new(Color::rgb(27.into(), 100.into(), 242.into())),
            border_radius: Property::new(5.0),
            background: Property::new(Color::rgb(229.into(), 231.into(), 235.into())),
        }
    }
}

pub struct SliderInstance {
    base: BaseInstance,
}

impl InstanceNode for SliderInstance {
    fn instantiate(args: InstantiationArgs) -> Rc<Self>
    where
        Self: Sized,
    {
        Rc::new(Self {
            base: BaseInstance::new(
                args,
                InstanceFlags {
                    invisible_to_slot: false,
                    invisible_to_raycasting: false,
                    layer: Layer::Native,
                    is_component: false,
                },
            ),
        })
    }

    fn handle_mount(
        self: Rc<Self>,
        expanded_node: &Rc<ExpandedNode>,
        context: &Rc<RuntimeContext>,
    ) {
        // Send creation message
        let id = expanded_node.id.clone();
        context.enqueue_native_message(pax_message::NativeMessage::SliderCreate(AnyCreatePatch {
            id: id.to_u32(),
            parent_frame: expanded_node.parent_frame.get().map(|v| v.to_u32()),
            occlusion_layer_id: 0,
        }));

        // send update message when relevant properties change
        let weak_self_ref = Rc::downgrade(&expanded_node);
        let context = Rc::clone(context);
        let last_patch = Rc::new(RefCell::new(SliderPatch {
            id: id.to_u32(),
            ..Default::default()
        }));

        let deps: Vec<_> = borrow_mut!(expanded_node.properties_scope)
            .values()
            .cloned()
            .map(|v| v.get_untyped_property().clone())
            .chain([expanded_node.transform_and_bounds.untyped()])
            .collect();
        expanded_node
            .native_message_listener
            .replace_with(Property::computed(
                move || {
                    let Some(expanded_node) = weak_self_ref.upgrade() else {
                        unreachable!()
                    };
                    let id = expanded_node.id.clone();
                    let mut old_state = borrow_mut!(last_patch);

                    let mut patch = SliderPatch {
                        id: id.to_u32(),
                        ..Default::default()
                    };
                    expanded_node.with_properties_unwrapped(|properties: &mut Slider| {
                        let computed_tab = expanded_node.transform_and_bounds.get();
                        let (width, height) = computed_tab.bounds;
                        let updates = [
                            patch_if_needed(&mut old_state.size_x, &mut patch.size_x, width),
                            patch_if_needed(&mut old_state.size_y, &mut patch.size_y, height),
                            patch_if_needed(
                                &mut old_state.transform,
                                &mut patch.transform,
                                computed_tab.transform.coeffs().to_vec(),
                            ),
                            patch_if_needed(
                                &mut old_state.accent,
                                &mut patch.accent,
                                (&properties.accent.get()).into(),
                            ),
                            patch_if_needed(
                                &mut old_state.value,
                                &mut patch.value,
                                properties.value.get(),
                            ),
                            patch_if_needed(
                                &mut old_state.step,
                                &mut patch.step,
                                properties.step.get(),
                            ),
                            patch_if_needed(
                                &mut old_state.min,
                                &mut patch.min,
                                properties.min.get(),
                            ),
                            patch_if_needed(
                                &mut old_state.max,
                                &mut patch.max,
                                properties.max.get(),
                            ),
                            patch_if_needed(
                                &mut old_state.border_radius,
                                &mut patch.border_radius,
                                properties.border_radius.get(),
                            ),
                            patch_if_needed(
                                &mut old_state.background,
                                &mut patch.background,
                                (&properties.background.get()).into(),
                            ),
                        ];
                        if updates.into_iter().any(|v| v == true) {
                            context.enqueue_native_message(
                                pax_message::NativeMessage::SliderUpdate(patch),
                            );
                        }
                    });
                    ()
                },
                &deps,
            ));
    }

    fn handle_unmount(&self, expanded_node: &Rc<ExpandedNode>, context: &Rc<RuntimeContext>) {
        let id = expanded_node.id.clone();
        expanded_node
            .native_message_listener
            .replace_with(Property::default());
        context.enqueue_native_message(pax_message::NativeMessage::SliderDelete(id.to_u32()));
    }

    fn base(&self) -> &BaseInstance {
        &self.base
    }

    fn resolve_debug(
        &self,
        f: &mut std::fmt::Formatter,
        _expanded_node: Option<&ExpandedNode>,
    ) -> std::fmt::Result {
        f.debug_struct("Slider").finish_non_exhaustive()
    }

    fn handle_native_interrupt(
        &self,
        expanded_node: &Rc<ExpandedNode>,
        interrupt: &NativeInterrupt,
    ) {
        if let NativeInterrupt::FormSliderChange(args) = interrupt {
            expanded_node
                .with_properties_unwrapped(|props: &mut Slider| props.value.set(args.value));
        }
    }
}
