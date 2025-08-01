/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use app_units::Au;
use atomic_refcell::{AtomicRef, AtomicRefCell};
use style::Zero;
use style::properties::ComputedValues;
use style::values::specified::align::AlignFlags;
use style::values::specified::box_::DisplayInside;
use taffy::style_helpers::{TaffyMaxContent, TaffyMinContent};
use taffy::{AvailableSpace, MaybeMath, RequestedAxis, RunMode};

use super::{
    SpecificTaffyGridInfo, TaffyContainer, TaffyItemBox, TaffyItemBoxInner, TaffyStyloStyle,
};
use crate::cell::ArcRefCell;
use crate::context::LayoutContext;
use crate::formatting_contexts::{Baselines, IndependentFormattingContext};
use crate::fragment_tree::{
    BoxFragment, CollapsedBlockMargins, Fragment, FragmentFlags, SpecificLayoutInfo,
};
use crate::geom::{
    LazySize, LogicalVec2, PhysicalPoint, PhysicalRect, PhysicalSides, PhysicalSize, SizeConstraint,
};
use crate::layout_box_base::CacheableLayoutResult;
use crate::positioned::{AbsolutelyPositionedBox, PositioningContext, PositioningContextLength};
use crate::sizing::{ComputeInlineContentSizes, ContentSizes, InlineContentSizesResult};
use crate::style_ext::LayoutStyle;
use crate::{ConstraintSpace, ContainingBlock, ContainingBlockSize};

const DUMMY_NODE_ID: taffy::NodeId = taffy::NodeId::new(u64::MAX);

fn resolve_content_size(constraint: AvailableSpace, content_sizes: ContentSizes) -> f32 {
    match constraint {
        AvailableSpace::Definite(limit) => {
            let min = content_sizes.min_content.to_f32_px();
            let max = content_sizes.max_content.to_f32_px();
            limit.min(max).max(min)
        },
        AvailableSpace::MinContent => content_sizes.min_content.to_f32_px(),
        AvailableSpace::MaxContent => content_sizes.max_content.to_f32_px(),
    }
}

#[inline(always)]
fn with_independant_formatting_context<T>(
    item: &mut TaffyItemBoxInner,
    cb: impl FnOnce(&IndependentFormattingContext) -> T,
) -> T {
    match item {
        TaffyItemBoxInner::InFlowBox(context) => cb(context),
        TaffyItemBoxInner::OutOfFlowAbsolutelyPositionedBox(abspos_box) => {
            cb(&AtomicRefCell::borrow(abspos_box).context)
        },
    }
}

/// Layout parameters and intermediate results about a taffy container,
/// grouped to avoid passing around many parameters
struct TaffyContainerContext<'a> {
    source_child_nodes: &'a [ArcRefCell<TaffyItemBox>],
    layout_context: &'a LayoutContext<'a>,
    positioning_context: &'a mut PositioningContext,
    content_box_size_override: &'a ContainingBlock<'a>,
    style: &'a ComputedValues,
    specific_layout_info: Option<SpecificLayoutInfo>,

    /// Temporary location for children specific info, which will be moved into child fragments
    child_specific_layout_infos: Vec<Option<SpecificLayoutInfo>>,
}

struct ChildIter(std::ops::Range<usize>);
impl Iterator for ChildIter {
    type Item = taffy::NodeId;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(taffy::NodeId::from)
    }
}

impl taffy::TraversePartialTree for TaffyContainerContext<'_> {
    type ChildIter<'a>
        = ChildIter
    where
        Self: 'a;

    fn child_ids(&self, _node_id: taffy::NodeId) -> Self::ChildIter<'_> {
        ChildIter(0..self.source_child_nodes.len())
    }

    fn child_count(&self, _node_id: taffy::NodeId) -> usize {
        self.source_child_nodes.len()
    }

    fn get_child_id(&self, _node_id: taffy::NodeId, index: usize) -> taffy::NodeId {
        taffy::NodeId::from(index)
    }
}

impl taffy::LayoutPartialTree for TaffyContainerContext<'_> {
    type CoreContainerStyle<'a>
        = TaffyStyloStyle<&'a ComputedValues>
    where
        Self: 'a;

    fn get_core_container_style(&self, _node_id: taffy::NodeId) -> Self::CoreContainerStyle<'_> {
        TaffyStyloStyle::new(self.style, false /* is_replaced */)
    }

    fn set_unrounded_layout(&mut self, node_id: taffy::NodeId, layout: &taffy::Layout) {
        let id = usize::from(node_id);
        (*self.source_child_nodes[id]).borrow_mut().taffy_layout = *layout;
    }

    fn compute_child_layout(
        &mut self,
        node_id: taffy::NodeId,
        inputs: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let mut child = (*self.source_child_nodes[usize::from(node_id)]).borrow_mut();
        let child = &mut *child;

        with_independant_formatting_context(
            &mut child.taffy_level_box,
            |independent_context| -> taffy::LayoutOutput {
                // TODO: re-evaluate sizing constraint conversions in light of recent layout changes
                let containing_block = &self.content_box_size_override;
                let style = independent_context.style();
                let writing_mode = style.writing_mode;

                // Adjust known_dimensions from border box to content box
                let pbm = independent_context
                    .layout_style()
                    .padding_border_margin(containing_block);
                let pb_sum = pbm.padding_border_sums.map(|v| v.to_f32_px());
                let margin_sum = pbm.margin.auto_is(Au::zero).sum().map(|v| v.to_f32_px());
                let content_box_inset = pb_sum + margin_sum;
                let content_box_known_dimensions = taffy::Size {
                    width: inputs
                        .known_dimensions
                        .width
                        .map(|width| width - pb_sum.inline),
                    height: inputs
                        .known_dimensions
                        .height
                        .map(|height| height - pb_sum.block),
                };
                let preferred_aspect_ratio =
                    independent_context.preferred_aspect_ratio(&pbm.padding_border_sums);

                // TODO: pass min- and max- size
                let tentative_block_size = content_box_known_dimensions
                    .height
                    .map(Au::from_f32_px)
                    .map_or_else(SizeConstraint::default, SizeConstraint::Definite);

                // Compute inline size
                let inline_size = content_box_known_dimensions.width.unwrap_or_else(|| {
                    let constraint_space = ConstraintSpace {
                        block_size: tentative_block_size,
                        writing_mode,
                        preferred_aspect_ratio,
                    };

                    // TODO: pass min- and max- size
                    let result = independent_context
                        .inline_content_sizes(self.layout_context, &constraint_space);
                    let adjusted_available_space = inputs
                        .available_space
                        .width
                        .map_definite_value(|width| width - content_box_inset.inline);

                    resolve_content_size(adjusted_available_space, result.sizes)
                });

                // Return early if only inline content sizes are requested
                if inputs.run_mode == RunMode::ComputeSize &&
                    inputs.axis == RequestedAxis::Horizontal
                {
                    return taffy::LayoutOutput::from_outer_size(taffy::Size {
                        width: inline_size + pb_sum.inline,
                        // If RequestedAxis is Horizontal then height will be ignored.
                        height: 0.0,
                    });
                }

                let content_box_size_override = ContainingBlock {
                    size: ContainingBlockSize {
                        inline: Au::from_f32_px(inline_size),
                        block: tentative_block_size,
                    },
                    style,
                };

                let lazy_block_size = match content_box_known_dimensions.height {
                    // FIXME: use the correct min/max sizes.
                    None => LazySize::intrinsic(),
                    Some(height) => Au::from_f32_px(height).into(),
                };

                child.positioning_context = PositioningContext::default();
                let layout = independent_context.layout_without_caching(
                    self.layout_context,
                    &mut child.positioning_context,
                    &content_box_size_override,
                    containing_block,
                    preferred_aspect_ratio,
                    &lazy_block_size,
                );

                child.child_fragments = layout.fragments;
                self.child_specific_layout_infos[usize::from(node_id)] =
                    layout.specific_layout_info;

                let block_size = lazy_block_size
                    .resolve(|| layout.content_block_size)
                    .to_f32_px();

                let computed_size = taffy::Size {
                    width: inline_size + pb_sum.inline,
                    height: block_size + pb_sum.block,
                };
                let size = inputs.known_dimensions.unwrap_or(computed_size);

                taffy::LayoutOutput {
                    size,
                    first_baselines: taffy::Point {
                        x: None,
                        y: layout.baselines.first.map(|au| au.to_f32_px()),
                    },
                    ..taffy::LayoutOutput::DEFAULT
                }
            },
        )
    }
}

impl taffy::LayoutGridContainer for TaffyContainerContext<'_> {
    type GridContainerStyle<'a>
        = TaffyStyloStyle<&'a ComputedValues>
    where
        Self: 'a;

    type GridItemStyle<'a>
        = TaffyStyloStyle<AtomicRef<'a, ComputedValues>>
    where
        Self: 'a;

    fn get_grid_container_style(
        &self,
        _node_id: taffy::prelude::NodeId,
    ) -> Self::GridContainerStyle<'_> {
        TaffyStyloStyle::new(self.style, false /* is_replaced */)
    }

    fn get_grid_child_style(
        &self,
        child_node_id: taffy::prelude::NodeId,
    ) -> Self::GridItemStyle<'_> {
        let id = usize::from(child_node_id);
        let child = (*self.source_child_nodes[id]).borrow();
        // TODO: account for non-replaced elements that are "compressible replaced"
        let is_replaced = child.is_in_flow_replaced();
        let stylo_style = AtomicRef::map(child, |c| &*c.style);
        TaffyStyloStyle::new(stylo_style, is_replaced)
    }

    fn set_detailed_grid_info(
        &mut self,
        _node_id: taffy::NodeId,
        specific_layout_info: taffy::DetailedGridInfo,
    ) {
        self.specific_layout_info = Some(SpecificLayoutInfo::Grid(Box::new(
            SpecificTaffyGridInfo::from_detailed_grid_layout(specific_layout_info),
        )));
    }
}

impl ComputeInlineContentSizes for TaffyContainer {
    fn compute_inline_content_sizes(
        &self,
        layout_context: &LayoutContext,
        _constraint_space: &ConstraintSpace,
    ) -> InlineContentSizesResult {
        let style = &self.style;

        let max_content_inputs = taffy::LayoutInput {
            run_mode: taffy::RunMode::ComputeSize,
            sizing_mode: taffy::SizingMode::InherentSize,
            axis: taffy::RequestedAxis::Horizontal,
            vertical_margins_are_collapsible: taffy::Line::FALSE,

            known_dimensions: taffy::Size::NONE,
            parent_size: taffy::Size::NONE,
            available_space: taffy::Size::MAX_CONTENT,
        };

        let min_content_inputs = taffy::LayoutInput {
            available_space: taffy::Size::MIN_CONTENT,
            ..max_content_inputs
        };

        let containing_block = &ContainingBlock {
            size: ContainingBlockSize {
                inline: Au::zero(),
                block: SizeConstraint::default(),
            },
            style,
        };

        let mut grid_context = TaffyContainerContext {
            layout_context,
            positioning_context: &mut PositioningContext::default(),
            content_box_size_override: containing_block,
            style,
            source_child_nodes: &self.children,
            specific_layout_info: None,
            child_specific_layout_infos: vec![None; self.children.len()],
        };

        let (max_content_output, min_content_output) = match style.clone_display().inside() {
            DisplayInside::Grid => {
                let max_content_output = taffy::compute_grid_layout(
                    &mut grid_context,
                    DUMMY_NODE_ID,
                    max_content_inputs,
                );
                let min_content_output = taffy::compute_grid_layout(
                    &mut grid_context,
                    DUMMY_NODE_ID,
                    min_content_inputs,
                );
                (max_content_output, min_content_output)
            },
            _ => panic!("Servo is only configured to use Taffy for CSS Grid layout"),
        };

        let pb_sums = self
            .layout_style()
            .padding_border_margin(containing_block)
            .padding_border_sums;

        InlineContentSizesResult {
            sizes: ContentSizes {
                max_content: Au::from_f32_px(max_content_output.size.width) - pb_sums.inline,
                min_content: Au::from_f32_px(min_content_output.size.width) - pb_sums.inline,
            },

            // TODO: determine this accurately
            //
            // "true" is a safe default as it will prevent Servo from performing optimizations based
            // on the assumption that the node's size does not depend on block constraints.
            depends_on_block_constraints: true,
        }
    }
}

impl TaffyContainer {
    /// <https://drafts.csswg.org/css-grid/#layout-algorithm>
    pub(crate) fn layout(
        &self,
        layout_context: &LayoutContext,
        positioning_context: &mut PositioningContext,
        content_box_size_override: &ContainingBlock,
        containing_block: &ContainingBlock,
    ) -> CacheableLayoutResult {
        let mut container_ctx = TaffyContainerContext {
            layout_context,
            positioning_context,
            content_box_size_override,
            style: content_box_size_override.style,
            source_child_nodes: &self.children,
            specific_layout_info: None,
            child_specific_layout_infos: vec![None; self.children.len()],
        };

        let container_style = &content_box_size_override.style;
        let align_items = container_style.clone_align_items();
        let justify_items = container_style.clone_justify_items();
        let pbm = self.layout_style().padding_border_margin(containing_block);

        let known_dimensions = taffy::Size {
            width: Some(
                (content_box_size_override.size.inline + pbm.padding_border_sums.inline)
                    .to_f32_px(),
            ),
            height: content_box_size_override
                .size
                .block
                .to_definite()
                .map(Au::to_f32_px)
                .maybe_add(pbm.padding_border_sums.block.to_f32_px()),
        };

        let taffy_containing_block = taffy::Size {
            width: Some(containing_block.size.inline.to_f32_px()),
            height: containing_block.size.block.to_definite().map(Au::to_f32_px),
        };

        let layout_input = taffy::LayoutInput {
            run_mode: taffy::RunMode::PerformLayout,
            sizing_mode: taffy::SizingMode::InherentSize,
            axis: taffy::RequestedAxis::Vertical,
            vertical_margins_are_collapsible: taffy::Line::FALSE,

            known_dimensions,
            parent_size: taffy_containing_block,
            available_space: taffy_containing_block.map(AvailableSpace::from),
        };

        let output = match container_ctx.style.clone_display().inside() {
            DisplayInside::Grid => {
                taffy::compute_grid_layout(&mut container_ctx, DUMMY_NODE_ID, layout_input)
            },
            _ => panic!("Servo is only configured to use Taffy for CSS Grid layout"),
        };

        // Convert `taffy::Layout` into Servo `Fragment`s
        // with container_ctx.child_specific_layout_infos will also moved to the corresponding `Fragment`s
        let fragments: Vec<Fragment> = self
            .children
            .iter()
            .map(|child| (**child).borrow_mut())
            .enumerate()
            .map(|(child_id, mut child)| {
                fn rect_to_physical_sides<T>(rect: taffy::Rect<T>) -> PhysicalSides<T> {
                    PhysicalSides::new(rect.top, rect.right, rect.bottom, rect.left)
                }

                fn size_and_pos_to_logical_rect<T: Default>(
                    position: taffy::Point<T>,
                    size: taffy::Size<T>,
                ) -> PhysicalRect<T> {
                    PhysicalRect::new(
                        PhysicalPoint::new(position.x, position.y),
                        PhysicalSize::new(size.width, size.height),
                    )
                }

                let layout = &child.taffy_layout;

                let padding = rect_to_physical_sides(layout.padding.map(Au::from_f32_px));
                let border = rect_to_physical_sides(layout.border.map(Au::from_f32_px));
                let margin = rect_to_physical_sides(layout.margin.map(Au::from_f32_px));

                // Compute content box size and position.
                //
                // For the x/y position we have to correct for the difference between the
                // content box and the border box for both the parent and the child.
                let content_size = size_and_pos_to_logical_rect(
                    taffy::Point {
                        x: Au::from_f32_px(
                            layout.location.x + layout.padding.left + layout.border.left,
                        ) - pbm.padding.inline_start -
                            pbm.border.inline_start,
                        y: Au::from_f32_px(
                            layout.location.y + layout.padding.top + layout.border.top,
                        ) - pbm.padding.block_start -
                            pbm.border.block_start,
                    },
                    taffy::Size {
                        width: layout.size.width -
                            layout.padding.left -
                            layout.padding.right -
                            layout.border.left -
                            layout.border.right,
                        height: layout.size.height -
                            layout.padding.top -
                            layout.padding.bottom -
                            layout.border.top -
                            layout.border.bottom,
                    }
                    .map(Au::from_f32_px),
                );

                let child_specific_layout_info: Option<SpecificLayoutInfo> =
                    std::mem::take(&mut container_ctx.child_specific_layout_infos[child_id]);

                let fragment = match &mut child.taffy_level_box {
                    TaffyItemBoxInner::InFlowBox(independent_box) => {
                        let mut fragment_info = independent_box.base_fragment_info();
                        fragment_info
                            .flags
                            .insert(FragmentFlags::IS_FLEX_OR_GRID_ITEM);
                        let mut box_fragment = BoxFragment::new(
                            fragment_info,
                            independent_box.style().clone(),
                            std::mem::take(&mut child.child_fragments),
                            content_size,
                            padding,
                            border,
                            margin,
                            child_specific_layout_info,
                        )
                        .with_baselines(Baselines {
                            first: output.first_baselines.y.map(Au::from_f32_px),
                            last: None,
                        });

                        child.positioning_context.layout_collected_children(
                            container_ctx.layout_context,
                            &mut box_fragment,
                        );
                        child
                            .positioning_context
                            .adjust_static_position_of_hoisted_fragments_with_offset(
                                &box_fragment.content_rect.origin.to_vector(),
                                PositioningContextLength::zero(),
                            );
                        container_ctx
                            .positioning_context
                            .append(std::mem::take(&mut child.positioning_context));

                        Fragment::Box(ArcRefCell::new(box_fragment))
                    },
                    TaffyItemBoxInner::OutOfFlowAbsolutelyPositionedBox(abs_pos_box) => {
                        fn resolve_alignment(value: AlignFlags, auto: AlignFlags) -> AlignFlags {
                            match value {
                                AlignFlags::AUTO => auto,
                                AlignFlags::NORMAL => AlignFlags::STRETCH,
                                value => value,
                            }
                        }

                        let hoisted_box = AbsolutelyPositionedBox::to_hoisted(
                            abs_pos_box.clone(),
                            PhysicalRect::from_size(PhysicalSize::new(
                                Au::from_f32_px(output.size.width),
                                Au::from_f32_px(output.size.height),
                            )),
                            LogicalVec2 {
                                inline: resolve_alignment(
                                    child.style.clone_align_self().0.0,
                                    align_items.0,
                                ),
                                block: resolve_alignment(
                                    child.style.clone_justify_self().0.0,
                                    justify_items.computed.0,
                                ),
                            },
                            container_ctx.style.writing_mode,
                        );
                        let hoisted_fragment = hoisted_box.fragment.clone();
                        container_ctx.positioning_context.push(hoisted_box);
                        Fragment::AbsoluteOrFixedPositioned(hoisted_fragment)
                    },
                };

                if let TaffyItemBoxInner::InFlowBox(independent_formatting_context) =
                    &child.taffy_level_box
                {
                    independent_formatting_context
                        .base
                        .set_fragment(fragment.clone());
                }
                fragment
            })
            .collect();

        CacheableLayoutResult {
            fragments,
            content_block_size: Au::from_f32_px(output.size.height) - pbm.padding_border_sums.block,
            content_inline_size_for_table: None,
            baselines: Baselines::default(),

            // TODO: determine this accurately
            //
            // "true" is a safe default as it will prevent Servo from performing optimizations based
            // on the assumption that the node's size does not depend on block constraints.
            depends_on_block_constraints: true,
            specific_layout_info: container_ctx.specific_layout_info,
            collapsible_margins_in_children: CollapsedBlockMargins::zero(),
        }
    }

    #[inline]
    pub(crate) fn layout_style(&self) -> LayoutStyle {
        LayoutStyle::Default(&self.style)
    }
}
