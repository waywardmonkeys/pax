use std::rc::Rc;
use std::{cell::RefCell, iter};

use crate::{
    BaseInstance, ExpandedNode, ExpressionTable, Globals, InstanceFlags, InstanceNode,
    InstanceNodePtrList, InstantiationArgs, RuntimeContext,
};
use pax_runtime_api::{Layer, Timeline};

/// A render node with its own runtime context.  Will push a frame
/// to the runtime stack including the specified `slot_children` and
/// a `dyn Any` properties object.  `Component` is used at the root of
/// applications, at the root of reusable components like `Stacker`, and
/// in special applications like `Repeat` where it houses the `RepeatItem`
/// properties attached to each of Repeat's virtual nodes.
pub struct ComponentInstance {
    pub template: InstanceNodePtrList,
    pub timeline: Option<Rc<RefCell<Timeline>>>,
    pub compute_properties_fn: Box<dyn Fn(&ExpandedNode, &ExpressionTable, &Globals)>,
    base: BaseInstance,
}

impl InstanceNode for ComponentInstance {
    fn instantiate(mut args: InstantiationArgs) -> Rc<Self> {
        let component_template = args.component_template.take();
        let template = component_template.unwrap_or_default();

        let compute_properties_fn = args.compute_properties_fn.take();
        let base = BaseInstance::new(
            args,
            InstanceFlags {
                invisible_to_slot: false,
                invisible_to_raycasting: true,
                layer: Layer::DontCare,
                is_component: true,
            },
        );
        Rc::new(ComponentInstance {
            base,
            template,
            compute_properties_fn: compute_properties_fn
                .expect("must pass a compute_properties_fn to a Component instance"),
            timeline: None,
        })
    }

    fn recompute_children(
        self: Rc<Self>,
        expanded_node: &Rc<ExpandedNode>,
        ptc: &mut RuntimeContext,
    ) {
        //change to expand children instead of self.template?
        let new_env = expanded_node.stack.push(&expanded_node.properties);
        let children_with_envs = self.template.iter().cloned().zip(iter::repeat(new_env));
        expanded_node.set_children(children_with_envs, ptc);

        // let expanded_and_flattened_slot_children = {
        //     let slot_children = self.base().get_children();
        //     //Expand children in the context of the current containing component
        //     let mut expanded_slot_children = vec![];
        //     for child in slot_children {
        //         let mut new_ptc = ptc.clone();
        //         let child_expanded_node = Rc::clone(&child).expand(&mut new_ptc);
        //         expanded_slot_children.push(child_expanded_node);
        //     }

        //     //Now flatten those expanded children, ignoring (replacing with children) and node that`is_invisible_to_slot`, namely
        //     //[`ConditionalInstance`] and [`RepeatInstance`]
        //     let mut expanded_and_flattened_slot_children = vec![];
        //     for expanded_slot_child in expanded_slot_children {
        //         expanded_and_flattened_slot_children.extend(flatten_expanded_node_for_slot(
        //             &Rc::clone(&expanded_slot_child),
        //         ));
        //     }

        //     expanded_and_flattened_slot_children
        // };

        // {
        //     this_expanded_node
        //         .borrow_mut()
        //         .set_expanded_and_flattened_slot_children(Some(
        //             expanded_and_flattened_slot_children,
        //         ));
        // }
    }

    fn update(&self, expanded_node: &Rc<ExpandedNode>, context: &mut RuntimeContext) {
        //Compute properties
        (*self.compute_properties_fn)(
            &expanded_node,
            context.expression_table(),
            context.globals(),
        );
    }

    #[cfg(debug_assertions)]
    fn resolve_debug(
        &self,
        f: &mut std::fmt::Formatter,
        _expanded_node: Option<&ExpandedNode>,
    ) -> std::fmt::Result {
        f.debug_struct("Component").finish()
    }

    fn base(&self) -> &BaseInstance {
        &self.base
    }
}

// Given some InstanceNodePtrList, distill away all "slot-invisible" nodes (namely, `if` and `for`)
// and return another InstanceNodePtrList with a flattened top-level list of nodes.
// Helper function that accepts a
// fn flatten_expanded_node_for_slot(node: &Rc<ExpandedNode>) -> Vec<Rc<ExpandedNode>> {
//     let mut result = vec![];

//     let is_invisible_to_slot = {
//         let instance_node_borrowed = Rc::clone(&node.instance_node);
//         instance_node_borrowed.base().flags().invisible_to_slot
//     };
//     if is_invisible_to_slot {
//         // If the node is invisible, recurse on its children
//         for child in node.borrow().get_children_expanded_nodes().iter() {
//             result.extend(flatten_expanded_node_for_slot(child));
//         }
//     } else {
//         // If the node is visible, add it to the result
//         result.push(Rc::clone(node));
//     }

//     result
// }
