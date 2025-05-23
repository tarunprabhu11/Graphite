#![allow(clippy::too_many_arguments)]

use super::document_node_definitions::{NODE_OVERRIDES, NodePropertiesContext};
use super::utility_types::FrontendGraphDataType;
use crate::messages::layout::utility_types::widget_prelude::*;
use crate::messages::portfolio::document::utility_types::network_interface::InputConnector;
use crate::messages::prelude::*;
use dyn_any::DynAny;
use glam::{DAffine2, DVec2, IVec2, UVec2};
use graph_craft::Type;
use graph_craft::document::value::TaggedValue;
use graph_craft::document::{DocumentNode, DocumentNodeImplementation, NodeId, NodeInput};
use graphene_core::raster::curve::Curve;
use graphene_core::raster::image::ImageFrameTable;
use graphene_core::raster::{
	BlendMode, CellularDistanceFunction, CellularReturnType, Color, DomainWarpType, FractalType, LuminanceCalculation, NoiseType, RedGreenBlue, RedGreenBlueAlpha, RelativeAbsolute,
	SelectiveColorChoice,
};
use graphene_core::text::Font;
use graphene_core::vector::misc::CentroidType;
use graphene_core::vector::style::{GradientType, LineCap, LineJoin};
use graphene_std::animation::RealTimeMode;
use graphene_std::application_io::TextureFrameTable;
use graphene_std::ops::XY;
use graphene_std::transform::Footprint;
use graphene_std::vector::VectorDataTable;
use graphene_std::vector::misc::ArcType;
use graphene_std::vector::misc::{BooleanOperation, GridType};
use graphene_std::vector::style::{Fill, FillChoice, FillType, GradientStops};
use graphene_std::{GraphicGroupTable, RasterFrame};

pub(crate) fn string_properties(text: &str) -> Vec<LayoutGroup> {
	let widget = TextLabel::new(text).widget_holder();
	vec![LayoutGroup::Row { widgets: vec![widget] }]
}

fn optionally_update_value<T>(value: impl Fn(&T) -> Option<TaggedValue> + 'static + Send + Sync, node_id: NodeId, input_index: usize) -> impl Fn(&T) -> Message + 'static + Send + Sync {
	move |input_value: &T| match value(input_value) {
		Some(value) => NodeGraphMessage::SetInputValue { node_id, input_index, value }.into(),
		_ => Message::NoOp,
	}
}

pub fn update_value<T>(value: impl Fn(&T) -> TaggedValue + 'static + Send + Sync, node_id: NodeId, input_index: usize) -> impl Fn(&T) -> Message + 'static + Send + Sync {
	optionally_update_value(move |v| Some(value(v)), node_id, input_index)
}

pub fn commit_value<T>(_: &T) -> Message {
	DocumentMessage::AddTransaction.into()
}

pub fn expose_widget(node_id: NodeId, index: usize, data_type: FrontendGraphDataType, exposed: bool) -> WidgetHolder {
	ParameterExposeButton::new()
		.exposed(exposed)
		.data_type(data_type)
		.tooltip("Expose this parameter as a node input in the graph")
		.on_update(move |_parameter| {
			NodeGraphMessage::ExposeInput {
				input_connector: InputConnector::node(node_id, index),
				set_to_exposed: !exposed,
				start_transaction: true,
			}
			.into()
		})
		.widget_holder()
}

// TODO: Remove this when we have proper entry row formatting that includes room for Assists.
pub fn add_blank_assist(widgets: &mut Vec<WidgetHolder>) {
	widgets.extend_from_slice(&[
		// Custom CSS specific to the Properties panel converts this Section separator into the width of an assist (24px).
		Separator::new(SeparatorType::Section).widget_holder(),
		// This last one is the separator after the 24px assist.
		Separator::new(SeparatorType::Unrelated).widget_holder(),
	]);
}

pub fn start_widgets(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, data_type: FrontendGraphDataType, blank_assist: bool) -> Vec<WidgetHolder> {
	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return vec![];
	};
	let mut widgets = vec![expose_widget(node_id, index, data_type, input.is_exposed()), TextLabel::new(name).tooltip(description).widget_holder()];
	if blank_assist {
		add_blank_assist(&mut widgets);
	}

	widgets
}

pub(crate) fn property_from_type(
	node_id: NodeId,
	index: usize,
	ty: &Type,
	number_options: (Option<f64>, Option<f64>, Option<(f64, f64)>),
	context: &mut NodePropertiesContext,
) -> Result<Vec<LayoutGroup>, Vec<LayoutGroup>> {
	let Some(name) = context.network_interface.input_name(&node_id, index, context.selection_network_path) else {
		log::warn!("A widget failed to be built for node {node_id}, index {index} because the input name could not be determined");
		return Err(vec![]);
	};
	let Some(description) = context.network_interface.input_description(&node_id, index, context.selection_network_path) else {
		log::warn!("A widget failed to be built for node {node_id}, index {index} because the input description could not be determined");
		return Err(vec![]);
	};
	let Some(network) = context.network_interface.nested_network(context.selection_network_path) else {
		log::warn!("A widget failed to be built for node {node_id}, index {index} because the network could not be determined");
		return Err(vec![]);
	};
	let Some(document_node) = network.nodes.get(&node_id) else {
		log::warn!("A widget failed to be built for node {node_id}, index {index} because the document node does not exist");
		return Err(vec![]);
	};

	let (mut number_min, mut number_max, range) = number_options;
	let mut number_input = NumberInput::default();
	if let Some((range_start, range_end)) = range {
		number_min = Some(range_start);
		number_max = Some(range_end);
		number_input = number_input.mode_range().min(range_start).max(range_end);
	}

	let min = |x: f64| number_min.unwrap_or(x);
	let max = |x: f64| number_max.unwrap_or(x);

	let mut extra_widgets = vec![];
	let widgets = match ty {
		Type::Concrete(concrete_type) => {
			match concrete_type.alias.as_ref().map(|x| x.as_ref()) {
				// Aliased types (ambiguous values)
				Some("Percentage") => number_widget(document_node, node_id, index, name, description, number_input.percentage().min(min(0.)).max(max(100.)), true).into(),
				Some("SignedPercentage") => number_widget(document_node, node_id, index, name, description, number_input.percentage().min(min(-100.)).max(max(100.)), true).into(),
				Some("Angle") => number_widget(
					document_node,
					node_id,
					index,
					name,
					description,
					number_input.mode_range().min(min(-180.)).max(max(180.)).unit("°"),
					true,
				)
				.into(),
				Some("PixelLength") => number_widget(document_node, node_id, index, name, description, number_input.min(min(0.)).unit(" px"), true).into(),
				Some("Length") => number_widget(document_node, node_id, index, name, description, number_input.min(min(0.)), true).into(),
				Some("Fraction") => number_widget(document_node, node_id, index, name, description, number_input.mode_range().min(min(0.)).max(max(1.)), true).into(),
				Some("IntegerCount") => number_widget(document_node, node_id, index, name, description, number_input.int().min(min(1.)), true).into(),
				Some("SeedValue") => number_widget(document_node, node_id, index, name, description, number_input.int().min(min(0.)), true).into(),
				Some("Resolution") => vec2_widget(document_node, node_id, index, name, description, "W", "H", " px", Some(64.), add_blank_assist),

				// For all other types, use TypeId-based matching
				_ => {
					use std::any::TypeId;
					match concrete_type.id {
						Some(x) if x == TypeId::of::<bool>() => bool_widget(document_node, node_id, index, name, description, CheckboxInput::default(), true).into(),
						Some(x) if x == TypeId::of::<f64>() => {
							number_widget(document_node, node_id, index, name, description, number_input.min(min(f64::NEG_INFINITY)).max(max(f64::INFINITY)), true).into()
						}
						Some(x) if x == TypeId::of::<u32>() => {
							number_widget(document_node, node_id, index, name, description, number_input.int().min(min(0.)).max(max(f64::from(u32::MAX))), true).into()
						}
						Some(x) if x == TypeId::of::<u64>() => number_widget(document_node, node_id, index, name, description, number_input.int().min(min(0.)), true).into(),
						Some(x) if x == TypeId::of::<String>() => text_widget(document_node, node_id, index, name, description, true).into(),
						Some(x) if x == TypeId::of::<Color>() => color_widget(document_node, node_id, index, name, description, ColorInput::default().allow_none(false), true),
						Some(x) if x == TypeId::of::<Option<Color>>() => color_widget(document_node, node_id, index, name, description, ColorInput::default().allow_none(true), true),
						Some(x) if x == TypeId::of::<DVec2>() => vec2_widget(document_node, node_id, index, name, description, "X", "Y", "", None, add_blank_assist),
						Some(x) if x == TypeId::of::<UVec2>() => vec2_widget(document_node, node_id, index, name, description, "X", "Y", "", Some(0.), add_blank_assist),
						Some(x) if x == TypeId::of::<IVec2>() => vec2_widget(document_node, node_id, index, name, description, "X", "Y", "", None, add_blank_assist),
						Some(x) if x == TypeId::of::<Vec<f64>>() => vec_f64_input(document_node, node_id, index, name, description, TextInput::default(), true).into(),
						Some(x) if x == TypeId::of::<Vec<DVec2>>() => vec_dvec2_input(document_node, node_id, index, name, description, TextInput::default(), true).into(),
						Some(x) if x == TypeId::of::<Font>() => {
							let (font_widgets, style_widgets) = font_inputs(document_node, node_id, index, name, description, false);
							font_widgets.into_iter().chain(style_widgets.unwrap_or_default()).collect::<Vec<_>>().into()
						}
						Some(x) if x == TypeId::of::<Curve>() => curves_widget(document_node, node_id, index, name, description, true),
						Some(x) if x == TypeId::of::<GradientStops>() => color_widget(document_node, node_id, index, name, description, ColorInput::default().allow_none(false), true),
						Some(x) if x == TypeId::of::<VectorDataTable>() => vector_widget(document_node, node_id, index, name, description, true).into(),
						Some(x) if x == TypeId::of::<RasterFrame>() || x == TypeId::of::<ImageFrameTable<Color>>() || x == TypeId::of::<TextureFrameTable>() => {
							raster_widget(document_node, node_id, index, name, description, true).into()
						}
						Some(x) if x == TypeId::of::<GraphicGroupTable>() => group_widget(document_node, node_id, index, name, description, true).into(),
						Some(x) if x == TypeId::of::<Footprint>() => {
							let widgets = footprint_widget(document_node, node_id, index);
							let (last, rest) = widgets.split_last().expect("Footprint widget should return multiple rows");
							extra_widgets = rest.to_vec();
							last.clone()
						}
						Some(x) if x == TypeId::of::<BlendMode>() => blend_mode(document_node, node_id, index, name, description, true),
						Some(x) if x == TypeId::of::<RealTimeMode>() => real_time_mode(document_node, node_id, index, name, description, true),
						Some(x) if x == TypeId::of::<RedGreenBlue>() => color_channel(document_node, node_id, index, name, description, true),
						Some(x) if x == TypeId::of::<RedGreenBlueAlpha>() => rgba_channel(document_node, node_id, index, name, description, true),
						Some(x) if x == TypeId::of::<XY>() => xy_components(document_node, node_id, index, name, description, true),
						Some(x) if x == TypeId::of::<NoiseType>() => noise_type(document_node, node_id, index, name, description, true),
						Some(x) if x == TypeId::of::<FractalType>() => fractal_type(document_node, node_id, index, name, description, true, false),
						Some(x) if x == TypeId::of::<CellularDistanceFunction>() => cellular_distance_function(document_node, node_id, index, name, description, true, false),
						Some(x) if x == TypeId::of::<CellularReturnType>() => cellular_return_type(document_node, node_id, index, name, description, true, false),
						Some(x) if x == TypeId::of::<DomainWarpType>() => domain_warp_type(document_node, node_id, index, name, description, true, false),
						Some(x) if x == TypeId::of::<RelativeAbsolute>() => vec![
							DropdownInput::new(vec![vec![
								MenuListEntry::new("Relative")
									.label("Relative")
									.on_update(update_value(|_| TaggedValue::RelativeAbsolute(RelativeAbsolute::Relative), node_id, index)),
								MenuListEntry::new("Absolute")
									.label("Absolute")
									.on_update(update_value(|_| TaggedValue::RelativeAbsolute(RelativeAbsolute::Absolute), node_id, index)),
							]])
							.widget_holder(),
						]
						.into(),
						Some(x) if x == TypeId::of::<GridType>() => grid_type_widget(document_node, node_id, index, name, description, true),
						Some(x) if x == TypeId::of::<LineCap>() => line_cap_widget(document_node, node_id, index, name, description, true),
						Some(x) if x == TypeId::of::<LineJoin>() => line_join_widget(document_node, node_id, index, name, description, true),
						Some(x) if x == TypeId::of::<ArcType>() => arc_type_widget(document_node, node_id, index, name, description, true),
						Some(x) if x == TypeId::of::<FillType>() => vec![
							DropdownInput::new(vec![vec![
								MenuListEntry::new("Solid")
									.label("Solid")
									.on_update(update_value(|_| TaggedValue::FillType(FillType::Solid), node_id, index)),
								MenuListEntry::new("Gradient")
									.label("Gradient")
									.on_update(update_value(|_| TaggedValue::FillType(FillType::Gradient), node_id, index)),
							]])
							.widget_holder(),
						]
						.into(),
						Some(x) if x == TypeId::of::<GradientType>() => vec![
							DropdownInput::new(vec![vec![
								MenuListEntry::new("Linear")
									.label("Linear")
									.on_update(update_value(|_| TaggedValue::GradientType(GradientType::Linear), node_id, index)),
								MenuListEntry::new("Radial")
									.label("Radial")
									.on_update(update_value(|_| TaggedValue::GradientType(GradientType::Radial), node_id, index)),
							]])
							.widget_holder(),
						]
						.into(),
						Some(x) if x == TypeId::of::<BooleanOperation>() => boolean_operation_radio_buttons(document_node, node_id, index, name, description, true),
						Some(x) if x == TypeId::of::<CentroidType>() => centroid_widget(document_node, node_id, index),
						Some(x) if x == TypeId::of::<LuminanceCalculation>() => luminance_calculation(document_node, node_id, index, name, description, true),
						// Some(x) if x == TypeId::of::<ImaginateSamplingMethod>() => vec![
						// 	DropdownInput::new(
						// 		ImaginateSamplingMethod::list()
						// 			.into_iter()
						// 			.map(|method| {
						// 				vec![MenuListEntry::new(format!("{:?}", method)).label(method.to_string()).on_update(update_value(
						// 					move |_| TaggedValue::ImaginateSamplingMethod(method),
						// 					node_id,
						// 					index,
						// 				))]
						// 			})
						// 			.collect(),
						// 	)
						// 	.widget_holder(),
						// ]
						// .into(),
						// Some(x) if x == TypeId::of::<ImaginateMaskStartingFill>() => vec![
						// 	DropdownInput::new(
						// 		ImaginateMaskStartingFill::list()
						// 			.into_iter()
						// 			.map(|fill| {
						// 				vec![MenuListEntry::new(format!("{:?}", fill)).label(fill.to_string()).on_update(update_value(
						// 					move |_| TaggedValue::ImaginateMaskStartingFill(fill),
						// 					node_id,
						// 					index,
						// 				))]
						// 			})
						// 			.collect(),
						// 	)
						// 	.widget_holder(),
						// ]
						// .into(),
						_ => {
							let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, true);
							widgets.extend_from_slice(&[
								Separator::new(SeparatorType::Unrelated).widget_holder(),
								TextLabel::new("-")
									.tooltip(format!(
										"This data can only be supplied through the node graph because no widget exists for its type:\n\
										{}",
										concrete_type.name
									))
									.widget_holder(),
							]);
							return Err(vec![widgets.into()]);
						}
					}
				}
			}
		}
		Type::Generic(_) => vec![TextLabel::new("Generic type (not supported)").widget_holder()].into(),
		Type::Fn(_, out) => return property_from_type(node_id, index, out, number_options, context),
		Type::Future(out) => return property_from_type(node_id, index, out, number_options, context),
	};

	extra_widgets.push(widgets);

	Ok(extra_widgets)
}

pub fn text_widget(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> Vec<WidgetHolder> {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);

	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return vec![];
	};
	if let Some(TaggedValue::String(x)) = &input.as_non_exposed_value() {
		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			TextInput::new(x.clone())
				.on_update(update_value(|x: &TextInput| TaggedValue::String(x.value.clone()), node_id, index))
				.on_commit(commit_value)
				.widget_holder(),
		])
	}
	widgets
}

pub fn text_area_widget(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> Vec<WidgetHolder> {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);

	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return vec![];
	};
	if let Some(TaggedValue::String(x)) = &input.as_non_exposed_value() {
		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			TextAreaInput::new(x.clone())
				.on_update(update_value(|x: &TextAreaInput| TaggedValue::String(x.value.clone()), node_id, index))
				.on_commit(commit_value)
				.widget_holder(),
		])
	}
	widgets
}

pub fn bool_widget(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, checkbox_input: CheckboxInput, blank_assist: bool) -> Vec<WidgetHolder> {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);

	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return vec![];
	};
	if let Some(&TaggedValue::Bool(x)) = input.as_non_exposed_value() {
		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			checkbox_input
				.checked(x)
				.on_update(update_value(|x: &CheckboxInput| TaggedValue::Bool(x.checked), node_id, index))
				.on_commit(commit_value)
				.widget_holder(),
		])
	}
	widgets
}

pub fn footprint_widget(document_node: &DocumentNode, node_id: NodeId, index: usize) -> Vec<LayoutGroup> {
	let mut location_widgets = start_widgets(document_node, node_id, index, "Footprint", "TODO", FrontendGraphDataType::General, true);
	location_widgets.push(Separator::new(SeparatorType::Unrelated).widget_holder());

	let mut scale_widgets = vec![TextLabel::new("").widget_holder()];
	add_blank_assist(&mut scale_widgets);
	scale_widgets.push(Separator::new(SeparatorType::Unrelated).widget_holder());

	let mut resolution_widgets = vec![TextLabel::new("").widget_holder()];
	add_blank_assist(&mut resolution_widgets);
	resolution_widgets.push(Separator::new(SeparatorType::Unrelated).widget_holder());

	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return vec![];
	};
	if let Some(&TaggedValue::Footprint(footprint)) = input.as_non_exposed_value() {
		let top_left = footprint.transform.transform_point2(DVec2::ZERO);
		let bounds = footprint.scale();
		let oversample = footprint.resolution.as_dvec2() / bounds;

		location_widgets.extend_from_slice(&[
			NumberInput::new(Some(top_left.x))
				.label("X")
				.unit(" px")
				.on_update(update_value(
					move |x: &NumberInput| {
						let (offset, scale) = {
							let diff = DVec2::new(top_left.x - x.value.unwrap_or_default(), 0.);
							(top_left - diff, bounds)
						};

						let footprint = Footprint {
							transform: DAffine2::from_scale_angle_translation(scale, 0., offset),
							resolution: (oversample * scale).as_uvec2(),
							..footprint
						};

						TaggedValue::Footprint(footprint)
					},
					node_id,
					index,
				))
				.on_commit(commit_value)
				.widget_holder(),
			Separator::new(SeparatorType::Related).widget_holder(),
			NumberInput::new(Some(top_left.y))
				.label("Y")
				.unit(" px")
				.on_update(update_value(
					move |x: &NumberInput| {
						let (offset, scale) = {
							let diff = DVec2::new(0., top_left.y - x.value.unwrap_or_default());
							(top_left - diff, bounds)
						};

						let footprint = Footprint {
							transform: DAffine2::from_scale_angle_translation(scale, 0., offset),
							resolution: (oversample * scale).as_uvec2(),
							..footprint
						};

						TaggedValue::Footprint(footprint)
					},
					node_id,
					index,
				))
				.on_commit(commit_value)
				.widget_holder(),
		]);

		scale_widgets.extend_from_slice(&[
			NumberInput::new(Some(bounds.x))
				.label("W")
				.unit(" px")
				.on_update(update_value(
					move |x: &NumberInput| {
						let (offset, scale) = (top_left, DVec2::new(x.value.unwrap_or_default(), bounds.y));

						let footprint = Footprint {
							transform: DAffine2::from_scale_angle_translation(scale, 0., offset),
							resolution: (oversample * scale).as_uvec2(),
							..footprint
						};

						TaggedValue::Footprint(footprint)
					},
					node_id,
					index,
				))
				.on_commit(commit_value)
				.widget_holder(),
			Separator::new(SeparatorType::Related).widget_holder(),
			NumberInput::new(Some(bounds.y))
				.label("H")
				.unit(" px")
				.on_update(update_value(
					move |x: &NumberInput| {
						let (offset, scale) = (top_left, DVec2::new(bounds.x, x.value.unwrap_or_default()));

						let footprint = Footprint {
							transform: DAffine2::from_scale_angle_translation(scale, 0., offset),
							resolution: (oversample * scale).as_uvec2(),
							..footprint
						};

						TaggedValue::Footprint(footprint)
					},
					node_id,
					index,
				))
				.on_commit(commit_value)
				.widget_holder(),
		]);

		resolution_widgets.push(
			NumberInput::new(Some((footprint.resolution.as_dvec2() / bounds).x * 100.))
				.label("Resolution")
				.unit("%")
				.on_update(update_value(
					move |x: &NumberInput| {
						let resolution = (bounds * x.value.unwrap_or(100.) / 100.).as_uvec2().max((1, 1).into()).min((4000, 4000).into());

						let footprint = Footprint { resolution, ..footprint };
						TaggedValue::Footprint(footprint)
					},
					node_id,
					index,
				))
				.on_commit(commit_value)
				.widget_holder(),
		);
	}

	vec![
		LayoutGroup::Row { widgets: location_widgets },
		LayoutGroup::Row { widgets: scale_widgets },
		LayoutGroup::Row { widgets: resolution_widgets },
	]
}

pub fn vec2_widget(
	document_node: &DocumentNode,
	node_id: NodeId,
	index: usize,
	name: &str,
	description: &str,
	x: &str,
	y: &str,
	unit: &str,
	min: Option<f64>,
	mut assist: impl FnMut(&mut Vec<WidgetHolder>),
) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::Number, false);

	assist(&mut widgets);

	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	match input.as_non_exposed_value() {
		Some(&TaggedValue::DVec2(dvec2)) => {
			widgets.extend_from_slice(&[
				Separator::new(SeparatorType::Unrelated).widget_holder(),
				NumberInput::new(Some(dvec2.x))
					.label(x)
					.unit(unit)
					.min(min.unwrap_or(-((1_u64 << f64::MANTISSA_DIGITS) as f64)))
					.max((1_u64 << f64::MANTISSA_DIGITS) as f64)
					.on_update(update_value(move |input: &NumberInput| TaggedValue::DVec2(DVec2::new(input.value.unwrap(), dvec2.y)), node_id, index))
					.on_commit(commit_value)
					.widget_holder(),
				Separator::new(SeparatorType::Related).widget_holder(),
				NumberInput::new(Some(dvec2.y))
					.label(y)
					.unit(unit)
					.min(min.unwrap_or(-((1_u64 << f64::MANTISSA_DIGITS) as f64)))
					.max((1_u64 << f64::MANTISSA_DIGITS) as f64)
					.on_update(update_value(move |input: &NumberInput| TaggedValue::DVec2(DVec2::new(dvec2.x, input.value.unwrap())), node_id, index))
					.on_commit(commit_value)
					.widget_holder(),
			]);
		}
		Some(&TaggedValue::IVec2(ivec2)) => {
			let update_x = move |input: &NumberInput| TaggedValue::IVec2(IVec2::new(input.value.unwrap() as i32, ivec2.y));
			let update_y = move |input: &NumberInput| TaggedValue::IVec2(IVec2::new(ivec2.x, input.value.unwrap() as i32));
			widgets.extend_from_slice(&[
				Separator::new(SeparatorType::Unrelated).widget_holder(),
				NumberInput::new(Some(ivec2.x as f64))
					.int()
					.label(x)
					.unit(unit)
					.min(min.unwrap_or(-((1_u64 << f64::MANTISSA_DIGITS) as f64)))
					.max((1_u64 << f64::MANTISSA_DIGITS) as f64)
					.on_update(update_value(update_x, node_id, index))
					.on_commit(commit_value)
					.widget_holder(),
				Separator::new(SeparatorType::Related).widget_holder(),
				NumberInput::new(Some(ivec2.y as f64))
					.int()
					.label(y)
					.unit(unit)
					.min(min.unwrap_or(-((1_u64 << f64::MANTISSA_DIGITS) as f64)))
					.max((1_u64 << f64::MANTISSA_DIGITS) as f64)
					.on_update(update_value(update_y, node_id, index))
					.on_commit(commit_value)
					.widget_holder(),
			]);
		}
		Some(&TaggedValue::UVec2(uvec2)) => {
			let update_x = move |input: &NumberInput| TaggedValue::UVec2(UVec2::new(input.value.unwrap() as u32, uvec2.y));
			let update_y = move |input: &NumberInput| TaggedValue::UVec2(UVec2::new(uvec2.x, input.value.unwrap() as u32));
			widgets.extend_from_slice(&[
				Separator::new(SeparatorType::Unrelated).widget_holder(),
				NumberInput::new(Some(uvec2.x as f64))
					.int()
					.label(x)
					.unit(unit)
					.min(min.unwrap_or(0.))
					.max((1_u64 << f64::MANTISSA_DIGITS) as f64)
					.on_update(update_value(update_x, node_id, index))
					.on_commit(commit_value)
					.widget_holder(),
				Separator::new(SeparatorType::Related).widget_holder(),
				NumberInput::new(Some(uvec2.y as f64))
					.int()
					.label(y)
					.unit(unit)
					.min(min.unwrap_or(0.))
					.max((1_u64 << f64::MANTISSA_DIGITS) as f64)
					.on_update(update_value(update_y, node_id, index))
					.on_commit(commit_value)
					.widget_holder(),
			]);
		}
		Some(&TaggedValue::F64(value)) => {
			widgets.extend_from_slice(&[
				Separator::new(SeparatorType::Unrelated).widget_holder(),
				NumberInput::new(Some(value))
					.label(x)
					.unit(unit)
					.min(min.unwrap_or(-((1_u64 << f64::MANTISSA_DIGITS) as f64)))
					.max((1_u64 << f64::MANTISSA_DIGITS) as f64)
					.on_update(update_value(move |input: &NumberInput| TaggedValue::DVec2(DVec2::new(input.value.unwrap(), value)), node_id, index))
					.on_commit(commit_value)
					.widget_holder(),
				Separator::new(SeparatorType::Related).widget_holder(),
				NumberInput::new(Some(value))
					.label(y)
					.unit(unit)
					.min(min.unwrap_or(-((1_u64 << f64::MANTISSA_DIGITS) as f64)))
					.max((1_u64 << f64::MANTISSA_DIGITS) as f64)
					.on_update(update_value(move |input: &NumberInput| TaggedValue::DVec2(DVec2::new(value, input.value.unwrap())), node_id, index))
					.on_commit(commit_value)
					.widget_holder(),
			]);
		}
		_ => {}
	}

	LayoutGroup::Row { widgets }
}

pub fn vec_f64_input(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, text_input: TextInput, blank_assist: bool) -> Vec<WidgetHolder> {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::Number, blank_assist);

	let from_string = |string: &str| {
		string
			.split(&[',', ' '])
			.filter(|x| !x.is_empty())
			.map(str::parse::<f64>)
			.collect::<Result<Vec<_>, _>>()
			.ok()
			.map(TaggedValue::VecF64)
	};

	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return vec![];
	};
	if let Some(TaggedValue::VecF64(x)) = &input.as_non_exposed_value() {
		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			text_input
				.value(x.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", "))
				.on_update(optionally_update_value(move |x: &TextInput| from_string(&x.value), node_id, index))
				.widget_holder(),
		])
	}
	widgets
}

pub fn vec_dvec2_input(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, text_props: TextInput, blank_assist: bool) -> Vec<WidgetHolder> {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::Number, blank_assist);

	let from_string = |string: &str| {
		string
			.split(|c: char| !c.is_alphanumeric() && !matches!(c, '.' | '+' | '-'))
			.filter(|x| !x.is_empty())
			.map(|x| x.parse::<f64>().ok())
			.collect::<Option<Vec<_>>>()
			.map(|numbers| numbers.chunks_exact(2).map(|values| DVec2::new(values[0], values[1])).collect())
			.map(TaggedValue::VecDVec2)
	};

	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return vec![];
	};
	if let Some(TaggedValue::VecDVec2(x)) = &input.as_non_exposed_value() {
		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			text_props
				.value(x.iter().map(|v| format!("({}, {})", v.x, v.y)).collect::<Vec<_>>().join(", "))
				.on_update(optionally_update_value(move |x: &TextInput| from_string(&x.value), node_id, index))
				.widget_holder(),
		])
	}
	widgets
}

pub fn font_inputs(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> (Vec<WidgetHolder>, Option<Vec<WidgetHolder>>) {
	let mut first_widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);
	let mut second_widgets = None;

	let from_font_input = |font: &FontInput| TaggedValue::Font(Font::new(font.font_family.clone(), font.font_style.clone()));

	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return (vec![], None);
	};
	if let Some(TaggedValue::Font(font)) = &input.as_non_exposed_value() {
		first_widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			FontInput::new(font.font_family.clone(), font.font_style.clone())
				.on_update(update_value(from_font_input, node_id, index))
				.on_commit(commit_value)
				.widget_holder(),
		]);

		let mut second_row = vec![TextLabel::new("").widget_holder()];
		add_blank_assist(&mut second_row);
		second_row.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			FontInput::new(font.font_family.clone(), font.font_style.clone())
				.is_style_picker(true)
				.on_update(update_value(from_font_input, node_id, index))
				.on_commit(commit_value)
				.widget_holder(),
		]);
		second_widgets = Some(second_row);
	}
	(first_widgets, second_widgets)
}

pub fn vector_widget(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> Vec<WidgetHolder> {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::VectorData, blank_assist);

	widgets.push(Separator::new(SeparatorType::Unrelated).widget_holder());
	widgets.push(TextLabel::new("Vector data is supplied through the node graph").widget_holder());

	widgets
}

pub fn raster_widget(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> Vec<WidgetHolder> {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::Raster, blank_assist);

	widgets.push(Separator::new(SeparatorType::Unrelated).widget_holder());
	widgets.push(TextLabel::new("Raster data is supplied through the node graph").widget_holder());

	widgets
}

pub fn group_widget(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> Vec<WidgetHolder> {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::Group, blank_assist);

	widgets.push(Separator::new(SeparatorType::Unrelated).widget_holder());
	widgets.push(TextLabel::new("Group data is supplied through the node graph").widget_holder());

	widgets
}

pub fn number_widget(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, number_props: NumberInput, blank_assist: bool) -> Vec<WidgetHolder> {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::Number, blank_assist);

	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return vec![];
	};
	match input.as_non_exposed_value() {
		Some(&TaggedValue::F64(x)) => widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			number_props
				.value(Some(x))
				.on_update(update_value(move |x: &NumberInput| TaggedValue::F64(x.value.unwrap()), node_id, index))
				.on_commit(commit_value)
				.widget_holder(),
		]),
		Some(&TaggedValue::U32(x)) => widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			number_props
				.value(Some(x as f64))
				.on_update(update_value(move |x: &NumberInput| TaggedValue::U32((x.value.unwrap()) as u32), node_id, index))
				.on_commit(commit_value)
				.widget_holder(),
		]),
		Some(&TaggedValue::U64(x)) => widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			number_props
				.value(Some(x as f64))
				.on_update(update_value(move |x: &NumberInput| TaggedValue::U64((x.value.unwrap()) as u64), node_id, index))
				.on_commit(commit_value)
				.widget_holder(),
		]),
		Some(&TaggedValue::OptionalF64(x)) => {
			// TODO: Don't wipe out the previously set value (setting it back to the default of 100) when reenabling this checkbox back to Some from None
			let toggle_enabled = move |checkbox_input: &CheckboxInput| TaggedValue::OptionalF64(if checkbox_input.checked { Some(100.) } else { None });
			widgets.extend_from_slice(&[
				Separator::new(SeparatorType::Unrelated).widget_holder(),
				Separator::new(SeparatorType::Related).widget_holder(),
				// The checkbox toggles if the value is Some or None
				CheckboxInput::new(x.is_some())
					.on_update(update_value(toggle_enabled, node_id, index))
					.on_commit(commit_value)
					.widget_holder(),
				Separator::new(SeparatorType::Related).widget_holder(),
				Separator::new(SeparatorType::Unrelated).widget_holder(),
				number_props
					.value(x)
					.on_update(update_value(move |x: &NumberInput| TaggedValue::OptionalF64(x.value), node_id, index))
					.disabled(x.is_none())
					.on_commit(commit_value)
					.widget_holder(),
			]);
		}
		Some(&TaggedValue::DVec2(dvec2)) => widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			number_props
			// We use an arbitrary `y` instead of an arbitrary `x` here because the "Grid" node's "Spacing" value's height should be used from rectangular mode when transferred to "Y Spacing" in isometric mode
				.value(Some(dvec2.y))
				.on_update(update_value(move |x: &NumberInput| TaggedValue::F64(x.value.unwrap()), node_id, index))
				.on_commit(commit_value)
				.widget_holder(),
		]),
		_ => {}
	}

	widgets
}

// TODO: Generalize this instead of using a separate function per dropdown menu enum
pub fn color_channel(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);
	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(&TaggedValue::RedGreenBlue(mode)) = input.as_non_exposed_value() {
		let calculation_modes = [RedGreenBlue::Red, RedGreenBlue::Green, RedGreenBlue::Blue];
		let mut entries = Vec::with_capacity(calculation_modes.len());
		for method in calculation_modes {
			entries.push(
				MenuListEntry::new(format!("{method:?}"))
					.label(method.to_string())
					.on_update(update_value(move |_| TaggedValue::RedGreenBlue(method), node_id, index))
					.on_commit(commit_value),
			);
		}
		let entries = vec![entries];

		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			DropdownInput::new(entries).selected_index(Some(mode as u32)).widget_holder(),
		]);
	}
	LayoutGroup::Row { widgets }.with_tooltip("Color Channel")
}

pub fn real_time_mode(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);
	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(&TaggedValue::RealTimeMode(mode)) = input.as_non_exposed_value() {
		let calculation_modes = [
			RealTimeMode::Utc,
			RealTimeMode::Year,
			RealTimeMode::Hour,
			RealTimeMode::Minute,
			RealTimeMode::Second,
			RealTimeMode::Millisecond,
		];
		let mut entries = Vec::with_capacity(calculation_modes.len());
		for method in calculation_modes {
			entries.push(
				MenuListEntry::new(format!("{method:?}"))
					.label(method.to_string())
					.on_update(update_value(move |_| TaggedValue::RealTimeMode(method), node_id, index))
					.on_commit(commit_value),
			);
		}
		let entries = vec![entries];

		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			DropdownInput::new(entries).selected_index(Some(mode as u32)).widget_holder(),
		]);
	}
	LayoutGroup::Row { widgets }.with_tooltip("Real Time Mode")
}

pub fn rgba_channel(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);
	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(&TaggedValue::RedGreenBlueAlpha(mode)) = input.as_non_exposed_value() {
		let calculation_modes = [RedGreenBlueAlpha::Red, RedGreenBlueAlpha::Green, RedGreenBlueAlpha::Blue, RedGreenBlueAlpha::Alpha];
		let mut entries = Vec::with_capacity(calculation_modes.len());
		for method in calculation_modes {
			entries.push(
				MenuListEntry::new(format!("{method:?}"))
					.label(method.to_string())
					.on_update(update_value(move |_| TaggedValue::RedGreenBlueAlpha(method), node_id, index))
					.on_commit(commit_value),
			);
		}
		let entries = vec![entries];

		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			DropdownInput::new(entries).selected_index(Some(mode as u32)).widget_holder(),
		]);
	}
	LayoutGroup::Row { widgets }.with_tooltip("Color Channel")
}

pub fn xy_components(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);
	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(&TaggedValue::XY(mode)) = input.as_non_exposed_value() {
		let calculation_modes = [XY::X, XY::Y];
		let mut entries = Vec::with_capacity(calculation_modes.len());
		for method in calculation_modes {
			entries.push(
				MenuListEntry::new(format!("{method:?}"))
					.label(method.to_string())
					.on_update(update_value(move |_| TaggedValue::XY(method), node_id, index))
					.on_commit(commit_value),
			);
		}
		let entries = vec![entries];

		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			DropdownInput::new(entries).selected_index(Some(mode as u32)).widget_holder(),
		]);
	}
	LayoutGroup::Row { widgets }.with_tooltip("X or Y Component of Vector2")
}

// TODO: Generalize this instead of using a separate function per dropdown menu enum
pub fn noise_type(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);
	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(&TaggedValue::NoiseType(noise_type)) = input.as_non_exposed_value() {
		let entries = NoiseType::list()
			.iter()
			.map(|noise_type| {
				MenuListEntry::new(format!("{noise_type:?}"))
					.label(noise_type.to_string())
					.on_update(update_value(move |_| TaggedValue::NoiseType(*noise_type), node_id, index))
					.on_commit(commit_value)
			})
			.collect();

		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			DropdownInput::new(vec![entries]).selected_index(Some(noise_type as u32)).widget_holder(),
		]);
	}
	LayoutGroup::Row { widgets }.with_tooltip("Style of noise pattern")
}

// TODO: Generalize this instead of using a separate function per dropdown menu enum
pub fn fractal_type(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool, disabled: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);
	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(&TaggedValue::FractalType(fractal_type)) = input.as_non_exposed_value() {
		let entries = FractalType::list()
			.iter()
			.map(|fractal_type| {
				MenuListEntry::new(format!("{fractal_type:?}"))
					.label(fractal_type.to_string())
					.on_update(update_value(move |_| TaggedValue::FractalType(*fractal_type), node_id, index))
					.on_commit(commit_value)
			})
			.collect();

		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			DropdownInput::new(vec![entries]).selected_index(Some(fractal_type as u32)).disabled(disabled).widget_holder(),
		]);
	}
	LayoutGroup::Row { widgets }.with_tooltip("Style of layered levels of the noise pattern")
}

// TODO: Generalize this instead of using a separate function per dropdown menu enum
pub fn cellular_distance_function(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool, disabled: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);
	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(&TaggedValue::CellularDistanceFunction(cellular_distance_function)) = input.as_non_exposed_value() {
		let entries = CellularDistanceFunction::list()
			.iter()
			.map(|cellular_distance_function| {
				MenuListEntry::new(format!("{cellular_distance_function:?}"))
					.label(cellular_distance_function.to_string())
					.on_update(update_value(move |_| TaggedValue::CellularDistanceFunction(*cellular_distance_function), node_id, index))
					.on_commit(commit_value)
			})
			.collect();

		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			DropdownInput::new(vec![entries])
				.selected_index(Some(cellular_distance_function as u32))
				.disabled(disabled)
				.widget_holder(),
		]);
	}
	LayoutGroup::Row { widgets }.with_tooltip("Distance function used by the cellular noise")
}

// TODO: Generalize this instead of using a separate function per dropdown menu enum
pub fn cellular_return_type(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool, disabled: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);
	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(&TaggedValue::CellularReturnType(cellular_return_type)) = input.as_non_exposed_value() {
		let entries = CellularReturnType::list()
			.iter()
			.map(|cellular_return_type| {
				MenuListEntry::new(format!("{cellular_return_type:?}"))
					.label(cellular_return_type.to_string())
					.on_update(update_value(move |_| TaggedValue::CellularReturnType(*cellular_return_type), node_id, index))
					.on_commit(commit_value)
			})
			.collect();

		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			DropdownInput::new(vec![entries]).selected_index(Some(cellular_return_type as u32)).disabled(disabled).widget_holder(),
		]);
	}
	LayoutGroup::Row { widgets }.with_tooltip("Return type of the cellular noise")
}

// TODO: Generalize this instead of using a separate function per dropdown menu enum
pub fn domain_warp_type(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool, disabled: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);
	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(&TaggedValue::DomainWarpType(domain_warp_type)) = input.as_non_exposed_value() {
		let entries = DomainWarpType::list()
			.iter()
			.map(|domain_warp_type| {
				MenuListEntry::new(format!("{domain_warp_type:?}"))
					.label(domain_warp_type.to_string())
					.on_update(update_value(move |_| TaggedValue::DomainWarpType(*domain_warp_type), node_id, index))
					.on_commit(commit_value)
			})
			.collect();

		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			DropdownInput::new(vec![entries]).selected_index(Some(domain_warp_type as u32)).disabled(disabled).widget_holder(),
		]);
	}
	LayoutGroup::Row { widgets }.with_tooltip("Type of domain warp")
}

// TODO: Generalize this instead of using a separate function per dropdown menu enum
pub fn blend_mode(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);
	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(&TaggedValue::BlendMode(blend_mode)) = input.as_non_exposed_value() {
		let entries = BlendMode::list_svg_subset()
			.iter()
			.map(|category| {
				category
					.iter()
					.map(|blend_mode| {
						MenuListEntry::new(format!("{blend_mode:?}"))
							.label(blend_mode.to_string())
							.on_update(update_value(move |_| TaggedValue::BlendMode(*blend_mode), node_id, index))
							.on_commit(commit_value)
					})
					.collect()
			})
			.collect();

		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			DropdownInput::new(entries)
				.selected_index(blend_mode.index_in_list_svg_subset().map(|index| index as u32))
				.widget_holder(),
		]);
	}
	LayoutGroup::Row { widgets }.with_tooltip("Formula used for blending")
}

// TODO: Generalize this for all dropdowns (also see blend_mode and channel_extration)
pub fn luminance_calculation(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);
	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(&TaggedValue::LuminanceCalculation(calculation)) = input.as_non_exposed_value() {
		let calculation_modes = LuminanceCalculation::list();
		let mut entries = Vec::with_capacity(calculation_modes.len());
		for method in calculation_modes {
			entries.push(
				MenuListEntry::new(format!("{method:?}"))
					.label(method.to_string())
					.on_update(update_value(move |_| TaggedValue::LuminanceCalculation(method), node_id, index))
					.on_commit(commit_value),
			);
		}
		let entries = vec![entries];

		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			DropdownInput::new(entries).selected_index(Some(calculation as u32)).widget_holder(),
		]);
	}
	LayoutGroup::Row { widgets }.with_tooltip("Formula used to calculate the luminance of a pixel")
}

pub fn boolean_operation_radio_buttons(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);

	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(&TaggedValue::BooleanOperation(calculation)) = input.as_non_exposed_value() {
		let operations = BooleanOperation::list();
		let icons = BooleanOperation::icons();
		let mut entries = Vec::with_capacity(operations.len());

		for (operation, icon) in operations.into_iter().zip(icons.into_iter()) {
			entries.push(
				RadioEntryData::new(format!("{operation:?}"))
					.icon(icon)
					.tooltip(operation.to_string())
					.on_update(update_value(move |_| TaggedValue::BooleanOperation(operation), node_id, index))
					.on_commit(commit_value),
			);
		}

		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			RadioInput::new(entries).selected_index(Some(calculation as u32)).widget_holder(),
		]);
	}
	LayoutGroup::Row { widgets }
}

pub fn grid_type_widget(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);
	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(&TaggedValue::GridType(grid_type)) = input.as_non_exposed_value() {
		let entries = [("Rectangular", GridType::Rectangular), ("Isometric", GridType::Isometric)]
			.into_iter()
			.map(|(name, val)| {
				RadioEntryData::new(format!("{val:?}"))
					.label(name)
					.on_update(update_value(move |_| TaggedValue::GridType(val), node_id, index))
					.on_commit(commit_value)
			})
			.collect();

		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			RadioInput::new(entries).selected_index(Some(grid_type as u32)).widget_holder(),
		]);
	}
	LayoutGroup::Row { widgets }
}

pub fn line_cap_widget(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);
	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(&TaggedValue::LineCap(line_cap)) = input.as_non_exposed_value() {
		let entries = [("Butt", LineCap::Butt), ("Round", LineCap::Round), ("Square", LineCap::Square)]
			.into_iter()
			.map(|(name, val)| {
				RadioEntryData::new(format!("{val:?}"))
					.label(name)
					.on_update(update_value(move |_| TaggedValue::LineCap(val), node_id, index))
					.on_commit(commit_value)
			})
			.collect();

		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			RadioInput::new(entries).selected_index(Some(line_cap as u32)).widget_holder(),
		]);
	}
	LayoutGroup::Row { widgets }
}

pub fn line_join_widget(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);
	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(&TaggedValue::LineJoin(line_join)) = input.as_non_exposed_value() {
		let entries = [("Miter", LineJoin::Miter), ("Bevel", LineJoin::Bevel), ("Round", LineJoin::Round)]
			.into_iter()
			.map(|(name, val)| {
				RadioEntryData::new(format!("{val:?}"))
					.label(name)
					.on_update(update_value(move |_| TaggedValue::LineJoin(val), node_id, index))
					.on_commit(commit_value)
			})
			.collect();

		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			RadioInput::new(entries).selected_index(Some(line_join as u32)).widget_holder(),
		]);
	}
	LayoutGroup::Row { widgets }
}

pub fn arc_type_widget(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);
	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(&TaggedValue::ArcType(arc_type)) = input.as_non_exposed_value() {
		let entries = [("Open", ArcType::Open), ("Closed", ArcType::Closed), ("Pie Slice", ArcType::PieSlice)]
			.into_iter()
			.map(|(name, val)| {
				RadioEntryData::new(format!("{val:?}"))
					.label(name)
					.on_update(update_value(move |_| TaggedValue::ArcType(val), node_id, index))
					.on_commit(commit_value)
			})
			.collect();

		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			RadioInput::new(entries).selected_index(Some(arc_type as u32)).widget_holder(),
		]);
	}
	LayoutGroup::Row { widgets }
}

pub fn color_widget(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, color_button: ColorInput, blank_assist: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);

	// Return early with just the label if the input is exposed to the graph, meaning we don't want to show the color picker widget in the Properties panel
	let NodeInput::Value { tagged_value, exposed: false } = &document_node.inputs[index] else {
		return LayoutGroup::Row { widgets };
	};

	// Add a separator
	widgets.push(Separator::new(SeparatorType::Unrelated).widget_holder());

	// Add the color input
	match &**tagged_value {
		TaggedValue::Color(color) => widgets.push(
			color_button
				.value(FillChoice::Solid(*color))
				.on_update(update_value(|x: &ColorInput| TaggedValue::Color(x.value.as_solid().unwrap_or_default()), node_id, index))
				.on_commit(commit_value)
				.widget_holder(),
		),
		TaggedValue::OptionalColor(color) => widgets.push(
			color_button
				.value(match color {
					Some(color) => FillChoice::Solid(*color),
					None => FillChoice::None,
				})
				.on_update(update_value(|x: &ColorInput| TaggedValue::OptionalColor(x.value.as_solid()), node_id, index))
				.on_commit(commit_value)
				.widget_holder(),
		),
		TaggedValue::GradientStops(x) => widgets.push(
			color_button
				.value(FillChoice::Gradient(x.clone()))
				.on_update(update_value(
					|x: &ColorInput| TaggedValue::GradientStops(x.value.as_gradient().cloned().unwrap_or_default()),
					node_id,
					index,
				))
				.on_commit(commit_value)
				.widget_holder(),
		),
		_ => {}
	}

	LayoutGroup::Row { widgets }
}

pub fn curves_widget(document_node: &DocumentNode, node_id: NodeId, index: usize, name: &str, description: &str, blank_assist: bool) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, name, description, FrontendGraphDataType::General, blank_assist);

	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(TaggedValue::Curve(curve)) = &input.as_non_exposed_value() {
		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			CurveInput::new(curve.clone())
				.on_update(update_value(|x: &CurveInput| TaggedValue::Curve(x.value.clone()), node_id, index))
				.on_commit(commit_value)
				.widget_holder(),
		])
	}
	LayoutGroup::Row { widgets }
}

pub fn centroid_widget(document_node: &DocumentNode, node_id: NodeId, index: usize) -> LayoutGroup {
	let mut widgets = start_widgets(document_node, node_id, index, "Centroid Type", "TODO", FrontendGraphDataType::General, true);
	let Some(input) = document_node.inputs.get(index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return LayoutGroup::Row { widgets: vec![] };
	};
	if let Some(&TaggedValue::CentroidType(centroid_type)) = input.as_non_exposed_value() {
		let entries = vec![
			RadioEntryData::new("area")
				.label("Area")
				.tooltip("Center of mass for the interior area of the shape")
				.on_update(update_value(move |_| TaggedValue::CentroidType(CentroidType::Area), node_id, index))
				.on_commit(commit_value),
			RadioEntryData::new("length")
				.label("Length")
				.tooltip("Center of mass for the perimeter arc length of the shape")
				.on_update(update_value(move |_| TaggedValue::CentroidType(CentroidType::Length), node_id, index))
				.on_commit(commit_value),
		];

		widgets.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			RadioInput::new(entries)
				.selected_index(match centroid_type {
					CentroidType::Area => Some(0),
					CentroidType::Length => Some(1),
				})
				.widget_holder(),
		]);
	}
	LayoutGroup::Row { widgets }
}

pub fn get_document_node<'a>(node_id: NodeId, context: &'a NodePropertiesContext<'a>) -> Result<&'a DocumentNode, String> {
	let network = context
		.network_interface
		.nested_network(context.selection_network_path)
		.ok_or("network not found in get_document_node")?;
	network.nodes.get(&node_id).ok_or(format!("node {node_id} not found in get_document_node"))
}

pub fn query_node_and_input_info<'a>(node_id: NodeId, input_index: usize, context: &'a NodePropertiesContext<'a>) -> Result<(&'a DocumentNode, &'a str, &'a str), String> {
	let document_node = get_document_node(node_id, context)?;
	let input_name = context
		.network_interface
		.input_name(&node_id, input_index, context.selection_network_path)
		.ok_or("input name not found in query_node_and_input_info")?;
	let input_description = context
		.network_interface
		.input_description(&node_id, input_index, context.selection_network_path)
		.ok_or("input description not found in query_node_and_input_info")?;
	Ok((document_node, input_name, input_description))
}

pub fn query_noise_pattern_state(node_id: NodeId, context: &NodePropertiesContext) -> Result<(bool, bool, bool, bool, bool, bool), String> {
	let document_node = get_document_node(node_id, context)?;
	let current_noise_type = document_node.inputs.iter().find_map(|input| match input.as_value() {
		Some(&TaggedValue::NoiseType(noise_type)) => Some(noise_type),
		_ => None,
	});
	let current_fractal_type = document_node.inputs.iter().find_map(|input| match input.as_value() {
		Some(&TaggedValue::FractalType(fractal_type)) => Some(fractal_type),
		_ => None,
	});
	let current_domain_warp_type = document_node.inputs.iter().find_map(|input| match input.as_value() {
		Some(&TaggedValue::DomainWarpType(domain_warp_type)) => Some(domain_warp_type),
		_ => None,
	});
	let fractal_active = current_fractal_type != Some(FractalType::None);
	let coherent_noise_active = current_noise_type != Some(NoiseType::WhiteNoise);
	let cellular_noise_active = current_noise_type == Some(NoiseType::Cellular);
	let ping_pong_active = current_fractal_type == Some(FractalType::PingPong);
	let domain_warp_active = current_domain_warp_type != Some(DomainWarpType::None);
	let domain_warp_only_fractal_type_wrongly_active =
		!domain_warp_active && (current_fractal_type == Some(FractalType::DomainWarpIndependent) || current_fractal_type == Some(FractalType::DomainWarpProgressive));

	Ok((
		fractal_active,
		coherent_noise_active,
		cellular_noise_active,
		ping_pong_active,
		domain_warp_active,
		domain_warp_only_fractal_type_wrongly_active,
	))
}

pub fn query_assign_colors_randomize(node_id: NodeId, context: &NodePropertiesContext) -> Result<bool, String> {
	let document_node = get_document_node(node_id, context)?;
	// This is safe since the node is a proto node and the implementation cannot be changed.
	let randomize_index = 5;
	Ok(match document_node.inputs.get(randomize_index).and_then(|input| input.as_value()) {
		Some(TaggedValue::Bool(randomize_enabled)) => *randomize_enabled,
		_ => false,
	})
}

pub(crate) fn channel_mixer_properties(node_id: NodeId, context: &mut NodePropertiesContext) -> Vec<LayoutGroup> {
	let document_node = match get_document_node(node_id, context) {
		Ok(document_node) => document_node,
		Err(err) => {
			log::error!("Could not get document node in channel_mixer_properties: {err}");
			return Vec::new();
		}
	};

	// Monochrome
	let monochrome_index = 1;
	let monochrome = bool_widget(document_node, node_id, monochrome_index, "Monochrome", "TODO", CheckboxInput::default(), true);
	let is_monochrome = match document_node.inputs[monochrome_index].as_value() {
		Some(TaggedValue::Bool(monochrome_choice)) => *monochrome_choice,
		_ => false,
	};

	// Output channel choice
	let output_channel_index = 18;
	let mut output_channel = vec![TextLabel::new("Output Channel").widget_holder(), Separator::new(SeparatorType::Unrelated).widget_holder()];
	add_blank_assist(&mut output_channel);

	let Some(input) = document_node.inputs.get(output_channel_index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return vec![];
	};
	if let Some(&TaggedValue::RedGreenBlue(choice)) = input.as_non_exposed_value() {
		let entries = vec![
			RadioEntryData::new(format!("{:?}", RedGreenBlue::Red))
				.label(RedGreenBlue::Red.to_string())
				.on_update(update_value(|_| TaggedValue::RedGreenBlue(RedGreenBlue::Red), node_id, output_channel_index))
				.on_commit(commit_value),
			RadioEntryData::new(format!("{:?}", RedGreenBlue::Green))
				.label(RedGreenBlue::Green.to_string())
				.on_update(update_value(|_| TaggedValue::RedGreenBlue(RedGreenBlue::Green), node_id, output_channel_index))
				.on_commit(commit_value),
			RadioEntryData::new(format!("{:?}", RedGreenBlue::Blue))
				.label(RedGreenBlue::Blue.to_string())
				.on_update(update_value(|_| TaggedValue::RedGreenBlue(RedGreenBlue::Blue), node_id, output_channel_index))
				.on_commit(commit_value),
		];
		output_channel.extend([RadioInput::new(entries).selected_index(Some(choice as u32)).widget_holder()]);
	};

	let is_output_channel = match &document_node.inputs[output_channel_index].as_value() {
		Some(TaggedValue::RedGreenBlue(choice)) => choice,
		_ => {
			warn!("Channel Mixer node properties panel could not be displayed.");
			return vec![];
		}
	};

	// Channel values
	let (r, g, b, c) = match (is_monochrome, is_output_channel) {
		(true, _) => ((2, "Red", 40.), (3, "Green", 40.), (4, "Blue", 20.), (5, "Constant", 0.)),
		(false, RedGreenBlue::Red) => ((6, "(Red) Red", 100.), (7, "(Red) Green", 0.), (8, "(Red) Blue", 0.), (9, "(Red) Constant", 0.)),
		(false, RedGreenBlue::Green) => ((10, "(Green) Red", 0.), (11, "(Green) Green", 100.), (12, "(Green) Blue", 0.), (13, "(Green) Constant", 0.)),
		(false, RedGreenBlue::Blue) => ((14, "(Blue) Red", 0.), (15, "(Blue) Green", 0.), (16, "(Blue) Blue", 100.), (17, "(Blue) Constant", 0.)),
	};
	let red = number_widget(
		document_node,
		node_id,
		r.0,
		r.1,
		"TODO",
		NumberInput::default().mode_range().min(-200.).max(200.).value(Some(r.2)).unit("%"),
		true,
	);
	let green = number_widget(
		document_node,
		node_id,
		g.0,
		g.1,
		"TODO",
		NumberInput::default().mode_range().min(-200.).max(200.).value(Some(g.2)).unit("%"),
		true,
	);
	let blue = number_widget(
		document_node,
		node_id,
		b.0,
		b.1,
		"TODO",
		NumberInput::default().mode_range().min(-200.).max(200.).value(Some(b.2)).unit("%"),
		true,
	);
	let constant = number_widget(
		document_node,
		node_id,
		c.0,
		c.1,
		"TODO",
		NumberInput::default().mode_range().min(-200.).max(200.).value(Some(c.2)).unit("%"),
		true,
	);

	// Monochrome
	let mut layout = vec![LayoutGroup::Row { widgets: monochrome }];
	// Output channel choice
	if !is_monochrome {
		layout.push(LayoutGroup::Row { widgets: output_channel });
	};
	// Channel values
	layout.extend([
		LayoutGroup::Row { widgets: red },
		LayoutGroup::Row { widgets: green },
		LayoutGroup::Row { widgets: blue },
		LayoutGroup::Row { widgets: constant },
	]);
	layout
}

pub(crate) fn selective_color_properties(node_id: NodeId, context: &mut NodePropertiesContext) -> Vec<LayoutGroup> {
	let document_node = match get_document_node(node_id, context) {
		Ok(document_node) => document_node,
		Err(err) => {
			log::error!("Could not get document node in selective_color_properties: {err}");
			return Vec::new();
		}
	};
	// Colors choice
	let colors_index = 38;
	let mut colors = vec![TextLabel::new("Colors").widget_holder(), Separator::new(SeparatorType::Unrelated).widget_holder()];
	add_blank_assist(&mut colors);

	let Some(input) = document_node.inputs.get(colors_index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return vec![];
	};
	if let Some(&TaggedValue::SelectiveColorChoice(choice)) = input.as_non_exposed_value() {
		use SelectiveColorChoice::*;
		let entries = [[Reds, Yellows, Greens, Cyans, Blues, Magentas].as_slice(), [Whites, Neutrals, Blacks].as_slice()]
			.into_iter()
			.map(|section| {
				section
					.iter()
					.map(|choice| {
						MenuListEntry::new(format!("{choice:?}"))
							.label(choice.to_string())
							.on_update(update_value(move |_| TaggedValue::SelectiveColorChoice(*choice), node_id, colors_index))
							.on_commit(commit_value)
					})
					.collect()
			})
			.collect();
		colors.extend([DropdownInput::new(entries).selected_index(Some(choice as u32)).widget_holder()]);
	}

	let colors_choice_index = match &document_node.inputs[colors_index].as_value() {
		Some(TaggedValue::SelectiveColorChoice(choice)) => choice,
		_ => {
			warn!("Selective Color node properties panel could not be displayed.");
			return vec![];
		}
	};

	// CMYK
	let (c, m, y, k) = match colors_choice_index {
		SelectiveColorChoice::Reds => ((2, "(Reds) Cyan"), (3, "(Reds) Magenta"), (4, "(Reds) Yellow"), (5, "(Reds) Black")),
		SelectiveColorChoice::Yellows => ((6, "(Yellows) Cyan"), (7, "(Yellows) Magenta"), (8, "(Yellows) Yellow"), (9, "(Yellows) Black")),
		SelectiveColorChoice::Greens => ((10, "(Greens) Cyan"), (11, "(Greens) Magenta"), (12, "(Greens) Yellow"), (13, "(Greens) Black")),
		SelectiveColorChoice::Cyans => ((14, "(Cyans) Cyan"), (15, "(Cyans) Magenta"), (16, "(Cyans) Yellow"), (17, "(Cyans) Black")),
		SelectiveColorChoice::Blues => ((18, "(Blues) Cyan"), (19, "(Blues) Magenta"), (20, "(Blues) Yellow"), (21, "(Blues) Black")),
		SelectiveColorChoice::Magentas => ((22, "(Magentas) Cyan"), (23, "(Magentas) Magenta"), (24, "(Magentas) Yellow"), (25, "(Magentas) Black")),
		SelectiveColorChoice::Whites => ((26, "(Whites) Cyan"), (27, "(Whites) Magenta"), (28, "(Whites) Yellow"), (29, "(Whites) Black")),
		SelectiveColorChoice::Neutrals => ((30, "(Neutrals) Cyan"), (31, "(Neutrals) Magenta"), (32, "(Neutrals) Yellow"), (33, "(Neutrals) Black")),
		SelectiveColorChoice::Blacks => ((34, "(Blacks) Cyan"), (35, "(Blacks) Magenta"), (36, "(Blacks) Yellow"), (37, "(Blacks) Black")),
	};
	let cyan = number_widget(document_node, node_id, c.0, c.1, "TODO", NumberInput::default().mode_range().min(-100.).max(100.).unit("%"), true);
	let magenta = number_widget(document_node, node_id, m.0, m.1, "TODO", NumberInput::default().mode_range().min(-100.).max(100.).unit("%"), true);
	let yellow = number_widget(document_node, node_id, y.0, y.1, "TODO", NumberInput::default().mode_range().min(-100.).max(100.).unit("%"), true);
	let black = number_widget(document_node, node_id, k.0, k.1, "TODO", NumberInput::default().mode_range().min(-100.).max(100.).unit("%"), true);

	// Mode
	let mode_index = 1;
	let mut mode = start_widgets(document_node, node_id, mode_index, "Mode", "TODO", FrontendGraphDataType::General, true);
	mode.push(Separator::new(SeparatorType::Unrelated).widget_holder());

	let Some(input) = document_node.inputs.get(mode_index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return vec![];
	};
	if let Some(&TaggedValue::RelativeAbsolute(relative_or_absolute)) = input.as_non_exposed_value() {
		let entries = vec![
			RadioEntryData::new("relative")
				.label("Relative")
				.on_update(update_value(|_| TaggedValue::RelativeAbsolute(RelativeAbsolute::Relative), node_id, mode_index))
				.on_commit(commit_value),
			RadioEntryData::new("absolute")
				.label("Absolute")
				.on_update(update_value(|_| TaggedValue::RelativeAbsolute(RelativeAbsolute::Absolute), node_id, mode_index))
				.on_commit(commit_value),
		];
		mode.push(RadioInput::new(entries).selected_index(Some(relative_or_absolute as u32)).widget_holder());
	};

	vec![
		// Colors choice
		LayoutGroup::Row { widgets: colors },
		// CMYK
		LayoutGroup::Row { widgets: cyan },
		LayoutGroup::Row { widgets: magenta },
		LayoutGroup::Row { widgets: yellow },
		LayoutGroup::Row { widgets: black },
		// Mode
		LayoutGroup::Row { widgets: mode },
	]
}

#[cfg(feature = "gpu")]
pub(crate) fn _gpu_map_properties(document_node: &DocumentNode, node_id: NodeId, _context: &mut NodePropertiesContext) -> Vec<LayoutGroup> {
	let map = text_widget(document_node, node_id, 1, "Map", "TODO", true);

	vec![LayoutGroup::Row { widgets: map }]
}

pub(crate) fn grid_properties(node_id: NodeId, context: &mut NodePropertiesContext) -> Vec<LayoutGroup> {
	let grid_type_index = 1;
	let spacing_index = 2;
	let angles_index = 3;
	let rows_index = 4;
	let columns_index = 5;

	let document_node = match get_document_node(node_id, context) {
		Ok(document_node) => document_node,
		Err(err) => {
			log::error!("Could not get document node in exposure_properties: {err}");
			return Vec::new();
		}
	};
	let grid_type = grid_type_widget(document_node, node_id, grid_type_index, "Grid Type", "TODO", true);

	let mut widgets = vec![grid_type];

	let Some(grid_type_input) = document_node.inputs.get(grid_type_index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return vec![];
	};
	if let Some(&TaggedValue::GridType(grid_type)) = grid_type_input.as_non_exposed_value() {
		match grid_type {
			GridType::Rectangular => {
				let spacing = vec2_widget(document_node, node_id, spacing_index, "Spacing", "TODO", "W", "H", " px", Some(0.), add_blank_assist);
				widgets.push(spacing);
			}
			GridType::Isometric => {
				let spacing = LayoutGroup::Row {
					widgets: number_widget(document_node, node_id, spacing_index, "Spacing", "TODO", NumberInput::default().label("H").min(0.).unit(" px"), true),
				};
				let angles = vec2_widget(document_node, node_id, angles_index, "Angles", "TODO", "", "", "°", None, add_blank_assist);
				widgets.extend([spacing, angles]);
			}
		}
	}

	let rows = number_widget(document_node, node_id, rows_index, "Rows", "TODO", NumberInput::default().min(1.), true);
	let columns = number_widget(document_node, node_id, columns_index, "Columns", "TODO", NumberInput::default().min(1.), true);

	widgets.extend([LayoutGroup::Row { widgets: rows }, LayoutGroup::Row { widgets: columns }]);

	widgets
}

pub(crate) fn exposure_properties(node_id: NodeId, context: &mut NodePropertiesContext) -> Vec<LayoutGroup> {
	let document_node = match get_document_node(node_id, context) {
		Ok(document_node) => document_node,
		Err(err) => {
			log::error!("Could not get document node in exposure_properties: {err}");
			return Vec::new();
		}
	};
	let exposure = number_widget(document_node, node_id, 1, "Exposure", "TODO", NumberInput::default().min(-20.).max(20.), true);
	let offset = number_widget(document_node, node_id, 2, "Offset", "TODO", NumberInput::default().min(-0.5).max(0.5), true);
	let gamma_input = NumberInput::default().min(0.01).max(9.99).increment_step(0.1);
	let gamma_correction = number_widget(document_node, node_id, 3, "Gamma Correction", "TODO", gamma_input, true);

	vec![
		LayoutGroup::Row { widgets: exposure },
		LayoutGroup::Row { widgets: offset },
		LayoutGroup::Row { widgets: gamma_correction },
	]
}

pub(crate) fn rectangle_properties(node_id: NodeId, context: &mut NodePropertiesContext) -> Vec<LayoutGroup> {
	let document_node = match get_document_node(node_id, context) {
		Ok(document_node) => document_node,
		Err(err) => {
			log::error!("Could not get document node in rectangle_properties: {err}");
			return Vec::new();
		}
	};
	let size_x_index = 1;
	let size_y_index = 2;
	let corner_rounding_type_index = 3;
	let corner_radius_index = 4;
	let clamped_index = 5;

	// Size X
	let size_x = number_widget(document_node, node_id, size_x_index, "Size X", "TODO", NumberInput::default(), true);

	// Size Y
	let size_y = number_widget(document_node, node_id, size_y_index, "Size Y", "TODO", NumberInput::default(), true);

	// Corner Radius
	let mut corner_radius_row_1 = start_widgets(document_node, node_id, corner_radius_index, "Corner Radius", "TODO", FrontendGraphDataType::Number, true);
	corner_radius_row_1.push(Separator::new(SeparatorType::Unrelated).widget_holder());

	let mut corner_radius_row_2 = vec![Separator::new(SeparatorType::Unrelated).widget_holder()];
	corner_radius_row_2.push(TextLabel::new("").widget_holder());
	add_blank_assist(&mut corner_radius_row_2);

	let Some(input) = document_node.inputs.get(corner_rounding_type_index) else {
		log::warn!("A widget failed to be built because its node's input index is invalid.");
		return vec![];
	};
	if let Some(&TaggedValue::Bool(is_individual)) = input.as_non_exposed_value() {
		// Values
		let Some(input) = document_node.inputs.get(corner_radius_index) else {
			log::warn!("A widget failed to be built because its node's input index is invalid.");
			return vec![];
		};
		let uniform_val = match input.as_non_exposed_value() {
			Some(TaggedValue::F64(x)) => *x,
			Some(TaggedValue::F64Array4(x)) => x[0],
			_ => 0.,
		};
		let individual_val = match input.as_non_exposed_value() {
			Some(&TaggedValue::F64Array4(x)) => x,
			Some(&TaggedValue::F64(x)) => [x; 4],
			_ => [0.; 4],
		};

		// Uniform/individual radio input widget
		let uniform = RadioEntryData::new("Uniform")
			.label("Uniform")
			.on_update(move |_| {
				Message::Batched(Box::new([
					NodeGraphMessage::SetInputValue {
						node_id,
						input_index: corner_rounding_type_index,
						value: TaggedValue::Bool(false),
					}
					.into(),
					NodeGraphMessage::SetInputValue {
						node_id,
						input_index: corner_radius_index,
						value: TaggedValue::F64(uniform_val),
					}
					.into(),
				]))
			})
			.on_commit(commit_value);
		let individual = RadioEntryData::new("Individual")
			.label("Individual")
			.on_update(move |_| {
				Message::Batched(Box::new([
					NodeGraphMessage::SetInputValue {
						node_id,
						input_index: corner_rounding_type_index,
						value: TaggedValue::Bool(true),
					}
					.into(),
					NodeGraphMessage::SetInputValue {
						node_id,
						input_index: corner_radius_index,
						value: TaggedValue::F64Array4(individual_val),
					}
					.into(),
				]))
			})
			.on_commit(commit_value);
		let radio_input = RadioInput::new(vec![uniform, individual]).selected_index(Some(is_individual as u32)).widget_holder();
		corner_radius_row_1.push(radio_input);

		// Radius value input widget
		let input_widget = if is_individual {
			let from_string = |string: &str| {
				string
					.split(&[',', ' '])
					.filter(|x| !x.is_empty())
					.map(str::parse::<f64>)
					.collect::<Result<Vec<f64>, _>>()
					.ok()
					.map(|v| {
						let arr: Box<[f64; 4]> = v.into_boxed_slice().try_into().unwrap_or_default();
						*arr
					})
					.map(TaggedValue::F64Array4)
			};
			TextInput::default()
				.value(individual_val.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", "))
				.on_update(optionally_update_value(move |x: &TextInput| from_string(&x.value), node_id, corner_radius_index))
				.widget_holder()
		} else {
			NumberInput::default()
				.value(Some(uniform_val))
				.on_update(update_value(move |x: &NumberInput| TaggedValue::F64(x.value.unwrap()), node_id, corner_radius_index))
				.on_commit(commit_value)
				.widget_holder()
		};
		corner_radius_row_2.push(input_widget);
	}

	// Clamped
	let clamped = bool_widget(document_node, node_id, clamped_index, "Clamped", "TODO", CheckboxInput::default(), true);

	vec![
		LayoutGroup::Row { widgets: size_x },
		LayoutGroup::Row { widgets: size_y },
		LayoutGroup::Row { widgets: corner_radius_row_1 },
		LayoutGroup::Row { widgets: corner_radius_row_2 },
		LayoutGroup::Row { widgets: clamped },
	]
}

// pub(crate) fn imaginate_properties(node_id: NodeId, context: &mut NodePropertiesContext) -> Vec<LayoutGroup> {
// 	let imaginate_node = [context.selection_network_path, &[node_id]].concat();

// 	let resolve_input = |name: &str| {
// 		IMAGINATE_NODE
// 			.default_node_template()
// 			.persistent_node_metadata
// 			.input_properties
// 			.iter()
// 			.position(|row| row.input_data.get("input_name").and_then(|v| v.as_str()) == Some(name))
// 			.unwrap_or_else(|| panic!("Input {name} not found"))
// 	};
// 	let seed_index = resolve_input("Seed");
// 	let resolution_index = resolve_input("Resolution");
// 	let samples_index = resolve_input("Samples");
// 	let sampling_method_index = resolve_input("Sampling Method");
// 	let text_guidance_index = resolve_input("Prompt Guidance");
// 	let text_index = resolve_input("Prompt");
// 	let neg_index = resolve_input("Negative Prompt");
// 	let base_img_index = resolve_input("Adapt Input Image");
// 	let img_creativity_index = resolve_input("Image Creativity");
// 	// let mask_index = resolve_input("Masking Layer");
// 	// let inpaint_index = resolve_input("Inpaint");
// 	// let mask_blur_index = resolve_input("Mask Blur");
// 	// let mask_fill_index = resolve_input("Mask Starting Fill");
// 	let faces_index = resolve_input("Improve Faces");
// 	let tiling_index = resolve_input("Tiling");

// 	let document_node = match get_document_node(node_id, context) {
// 		Ok(document_node) => document_node,
// 		Err(err) => {
// 			log::error!("Could not get document node in imaginate_properties: {err}");
// 			return Vec::new();
// 		}
// 	};
// 	let controller = &document_node.inputs[resolve_input("Controller")];

// 	let server_status = {
// 		let server_status = context.persistent_data.imaginate.server_status();
// 		let status_text = server_status.to_text();
// 		let mut widgets = vec![
// 			TextLabel::new("Server").widget_holder(),
// 			Separator::new(SeparatorType::Unrelated).widget_holder(),
// 			IconButton::new("Settings", 24)
// 				.tooltip("Preferences: Imaginate")
// 				.on_update(|_| DialogMessage::RequestPreferencesDialog.into())
// 				.widget_holder(),
// 			Separator::new(SeparatorType::Unrelated).widget_holder(),
// 			TextLabel::new(status_text).bold(true).widget_holder(),
// 			Separator::new(SeparatorType::Related).widget_holder(),
// 			IconButton::new("Reload", 24)
// 				.tooltip("Refresh connection status")
// 				.on_update(|_| PortfolioMessage::ImaginateCheckServerStatus.into())
// 				.widget_holder(),
// 		];
// 		if let ImaginateServerStatus::Unavailable | ImaginateServerStatus::Failed(_) = server_status {
// 			widgets.extend([
// 				Separator::new(SeparatorType::Unrelated).widget_holder(),
// 				TextButton::new("Server Help")
// 					.tooltip("Learn how to connect Imaginate to an image generation server")
// 					.on_update(|_| {
// 						FrontendMessage::TriggerVisitLink {
// 							url: "https://github.com/GraphiteEditor/Graphite/discussions/1089".to_string(),
// 						}
// 						.into()
// 					})
// 					.widget_holder(),
// 			]);
// 		}
// 		LayoutGroup::Row { widgets }.with_tooltip("Connection status to the server that computes generated images")
// 	};

// 	let Some(TaggedValue::ImaginateController(controller)) = controller.as_value() else {
// 		panic!("Invalid output status input")
// 	};
// 	let imaginate_status = controller.get_status();

// 	let use_base_image = if let Some(&TaggedValue::Bool(use_base_image)) = &document_node.inputs[base_img_index].as_value() {
// 		use_base_image
// 	} else {
// 		true
// 	};

// 	let transform_not_connected = false;

// 	let progress = {
// 		let mut widgets = vec![TextLabel::new("Progress").widget_holder(), Separator::new(SeparatorType::Unrelated).widget_holder()];
// 		add_blank_assist(&mut widgets);
// 		let status = imaginate_status.to_text();
// 		widgets.push(TextLabel::new(status.as_ref()).bold(true).widget_holder());
// 		LayoutGroup::Row { widgets }.with_tooltip(match imaginate_status {
// 			ImaginateStatus::Failed(_) => status.as_ref(),
// 			_ => "When generating, the percentage represents how many sampling steps have so far been processed out of the target number",
// 		})
// 	};

// 	let image_controls = {
// 		let mut widgets = vec![TextLabel::new("Image").widget_holder(), Separator::new(SeparatorType::Unrelated).widget_holder()];

// 		match &imaginate_status {
// 			ImaginateStatus::Beginning | ImaginateStatus::Uploading => {
// 				add_blank_assist(&mut widgets);
// 				widgets.push(TextButton::new("Beginning...").tooltip("Sending image generation request to the server").disabled(true).widget_holder());
// 			}
// 			ImaginateStatus::Generating(_) => {
// 				add_blank_assist(&mut widgets);
// 				widgets.push(
// 					TextButton::new("Terminate")
// 						.tooltip("Cancel the in-progress image generation and keep the latest progress")
// 						.on_update({
// 							let controller = controller.clone();
// 							move |_| {
// 								controller.request_termination();
// 								Message::NoOp
// 							}
// 						})
// 						.widget_holder(),
// 				);
// 			}
// 			ImaginateStatus::Terminating => {
// 				add_blank_assist(&mut widgets);
// 				widgets.push(
// 					TextButton::new("Terminating...")
// 						.tooltip("Waiting on the final image generated after termination")
// 						.disabled(true)
// 						.widget_holder(),
// 				);
// 			}
// 			ImaginateStatus::Ready | ImaginateStatus::ReadyDone | ImaginateStatus::Terminated | ImaginateStatus::Failed(_) => widgets.extend_from_slice(&[
// 				IconButton::new("Random", 24)
// 					.tooltip("Generate with a new random seed")
// 					.on_update({
// 						let imaginate_node = imaginate_node.clone();
// 						let controller = controller.clone();
// 						move |_| {
// 							controller.trigger_regenerate();
// 							DocumentMessage::ImaginateRandom {
// 								imaginate_node: imaginate_node.clone(),
// 								then_generate: true,
// 							}
// 							.into()
// 						}
// 					})
// 					.widget_holder(),
// 				Separator::new(SeparatorType::Unrelated).widget_holder(),
// 				TextButton::new("Generate")
// 					.tooltip("Fill layer frame by generating a new image")
// 					.on_update({
// 						let controller = controller.clone();
// 						let imaginate_node = imaginate_node.clone();
// 						move |_| {
// 							controller.trigger_regenerate();
// 							DocumentMessage::ImaginateGenerate {
// 								imaginate_node: imaginate_node.clone(),
// 							}
// 							.into()
// 						}
// 					})
// 					.widget_holder(),
// 				Separator::new(SeparatorType::Related).widget_holder(),
// 				TextButton::new("Clear")
// 					.tooltip("Remove generated image from the layer frame")
// 					.disabled(!matches!(imaginate_status, ImaginateStatus::ReadyDone))
// 					.on_update({
// 						let controller = controller.clone();
// 						let imaginate_node = imaginate_node.clone();
// 						move |_| {
// 							controller.set_status(ImaginateStatus::Ready);
// 							DocumentMessage::ImaginateGenerate {
// 								imaginate_node: imaginate_node.clone(),
// 							}
// 							.into()
// 						}
// 					})
// 					.widget_holder(),
// 			]),
// 		}
// 		LayoutGroup::Row { widgets }.with_tooltip("Buttons that control the image generation process")
// 	};

// 	// Requires custom layout for the regenerate button
// 	let seed = {
// 		let mut widgets = start_widgets(document_node, node_id, seed_index, "Seed", FrontendGraphDataType::Number, false);

// 		let Some(input) = document_node.inputs.get(seed_index) else {
// 			log::warn!("A widget failed to be built because its node's input index is invalid.");
// 			return vec![];
// 		};
// 		if let Some(&TaggedValue::F64(seed)) = &input.as_non_exposed_value() {
// 			widgets.extend_from_slice(&[
// 				Separator::new(SeparatorType::Unrelated).widget_holder(),
// 				IconButton::new("Resync", 24)
// 					.tooltip("Set a new random seed")
// 					.on_update({
// 						let imaginate_node = imaginate_node.clone();
// 						move |_| {
// 							DocumentMessage::ImaginateRandom {
// 								imaginate_node: imaginate_node.clone(),
// 								then_generate: false,
// 							}
// 							.into()
// 						}
// 					})
// 					.widget_holder(),
// 				Separator::new(SeparatorType::Unrelated).widget_holder(),
// 				NumberInput::new(Some(seed))
// 					.int()
// 					.min(-((1_u64 << f64::MANTISSA_DIGITS) as f64))
// 					.max((1_u64 << f64::MANTISSA_DIGITS) as f64)
// 					.on_update(update_value(move |input: &NumberInput| TaggedValue::F64(input.value.unwrap()), node_id, seed_index))
// 					.on_commit(commit_value)
// 					.mode(NumberInputMode::Increment)
// 					.widget_holder(),
// 			])
// 		}
// 		// Note: Limited by f64. You cannot even have all the possible u64 values :)
// 		LayoutGroup::Row { widgets }.with_tooltip("Seed determines the random outcome, enabling limitless unique variations")
// 	};

// 	// let transform = context
// 	// 	.executor
// 	// 	.introspect_node_in_network(context.network, &imaginate_node, |network| network.inputs.first().copied(), |frame: &ImageFrame<Color>| frame.transform)
// 	// 	.unwrap_or_default();
// 	let image_size = context
// 		.executor
// 		.introspect_node_in_network(
// 			context.network_interface.document_network().unwrap(),
// 			&imaginate_node,
// 			|network| {
// 				network
// 					.nodes
// 					.iter()
// 					.find(|node| {
// 						node.1
// 							.inputs
// 							.iter()
// 							.any(|node_input| if let NodeInput::Network { import_index, .. } = node_input { *import_index == 0 } else { false })
// 					})
// 					.map(|(node_id, _)| node_id)
// 					.copied()
// 			},
// 			|frame: &IORecord<(), ImageFrame<Color>>| (frame.output.image.width, frame.output.image.height),
// 		)
// 		.unwrap_or_default();

// 	let document_node = match get_document_node(node_id, context) {
// 		Ok(document_node) => document_node,
// 		Err(err) => {
// 			log::error!("Could not get document node in imaginate_properties: {err}");
// 			return Vec::new();
// 		}
// 	};

// 	let resolution = {
// 		let mut widgets = start_widgets(document_node, node_id, resolution_index, "Resolution", FrontendGraphDataType::Number, false);

// 		let round = |size: DVec2| {
// 			let (x, y) = graphene_std::imaginate::pick_safe_imaginate_resolution(size.into());
// 			DVec2::new(x as f64, y as f64)
// 		};

// 		let Some(input) = document_node.inputs.get(resolution_index) else {
// 			log::warn!("A widget failed to be built because its node's input index is invalid.");
// 			return vec![];
// 		};
// 		if let Some(&TaggedValue::OptionalDVec2(vec2)) = &input.as_non_exposed_value() {
// 			let dimensions_is_auto = vec2.is_none();
// 			let vec2 = vec2.unwrap_or_else(|| round((image_size.0 as f64, image_size.1 as f64).into()));

// 			widgets.extend_from_slice(&[
// 				Separator::new(SeparatorType::Unrelated).widget_holder(),
// 				IconButton::new("FrameAll", 24)
// 					.tooltip("Set the layer dimensions to this resolution")
// 					.on_update(move |_| DialogMessage::RequestComingSoonDialog { issue: None }.into())
// 					.widget_holder(),
// 				Separator::new(SeparatorType::Unrelated).widget_holder(),
// 				CheckboxInput::new(!dimensions_is_auto || transform_not_connected)
// 					.icon("Edit12px")
// 					.tooltip({
// 						let message = "Set a custom resolution instead of using the input's dimensions (rounded to the nearest 64)";
// 						let manual_message = "Set a custom resolution instead of using the input's dimensions (rounded to the nearest 64).\n\
// 							\n\
// 							(Resolution must be set manually while the 'Transform' input is disconnected.)";

// 						if transform_not_connected {
// 							manual_message
// 						} else {
// 							message
// 						}
// 					})
// 					.disabled(transform_not_connected)
// 					.on_update(update_value(
// 						move |checkbox_input: &CheckboxInput| TaggedValue::OptionalDVec2(if checkbox_input.checked { Some(vec2) } else { None }),
// 						node_id,
// 						resolution_index,
// 					))
// 					.on_commit(commit_value)
// 					.widget_holder(),
// 				Separator::new(SeparatorType::Related).widget_holder(),
// 				NumberInput::new(Some(vec2.x))
// 					.label("W")
// 					.min(64.)
// 					.step(64.)
// 					.unit(" px")
// 					.disabled(dimensions_is_auto && !transform_not_connected)
// 					.on_update(update_value(
// 						move |number_input: &NumberInput| TaggedValue::OptionalDVec2(Some(round(DVec2::new(number_input.value.unwrap(), vec2.y)))),
// 						node_id,
// 						resolution_index,
// 					))
// 					.on_commit(commit_value)
// 					.widget_holder(),
// 				Separator::new(SeparatorType::Related).widget_holder(),
// 				NumberInput::new(Some(vec2.y))
// 					.label("H")
// 					.min(64.)
// 					.step(64.)
// 					.unit(" px")
// 					.disabled(dimensions_is_auto && !transform_not_connected)
// 					.on_update(update_value(
// 						move |number_input: &NumberInput| TaggedValue::OptionalDVec2(Some(round(DVec2::new(vec2.x, number_input.value.unwrap())))),
// 						node_id,
// 						resolution_index,
// 					))
// 					.on_commit(commit_value)
// 					.widget_holder(),
// 			])
// 		}
// 		LayoutGroup::Row { widgets }.with_tooltip(
// 			"Width and height of the image that will be generated. Larger resolutions take longer to compute.\n\
// 			\n\
// 			512x512 yields optimal results because the AI is trained to understand that scale best. Larger sizes may tend to integrate the prompt's subject more than once. Small sizes are often incoherent.\n\
// 			\n\
// 			Dimensions must be a multiple of 64, so these are set by rounding the layer dimensions. A resolution exceeding 1 megapixel is reduced below that limit because larger sizes may exceed available GPU memory on the server.")
// 	};

// 	let sampling_steps = {
// 		let widgets = number_widget(document_node, node_id, samples_index, "Sampling Steps", NumberInput::default().min(0.).max(150.).int(), true);
// 		LayoutGroup::Row { widgets }.with_tooltip("Number of iterations to improve the image generation quality, with diminishing returns around 40 when using the Euler A sampling method")
// 	};

// 	let sampling_method = {
// 		let mut widgets = start_widgets(document_node, node_id, sampling_method_index, "Sampling Method", FrontendGraphDataType::General, true);

// 		let Some(input) = document_node.inputs.get(sampling_method_index) else {
// 			log::warn!("A widget failed to be built because its node's input index is invalid.");
// 			return vec![];
// 		};
// 		if let Some(&TaggedValue::ImaginateSamplingMethod(sampling_method)) = &input.as_non_exposed_value() {
// 			let sampling_methods = ImaginateSamplingMethod::list();
// 			let mut entries = Vec::with_capacity(sampling_methods.len());
// 			for method in sampling_methods {
// 				entries.push(
// 					MenuListEntry::new(format!("{method:?}"))
// 						.label(method.to_string())
// 						.on_update(update_value(move |_| TaggedValue::ImaginateSamplingMethod(method), node_id, sampling_method_index))
// 						.on_commit(commit_value),
// 				);
// 			}
// 			let entries = vec![entries];

// 			widgets.extend_from_slice(&[
// 				Separator::new(SeparatorType::Unrelated).widget_holder(),
// 				DropdownInput::new(entries).selected_index(Some(sampling_method as u32)).widget_holder(),
// 			]);
// 		}
// 		LayoutGroup::Row { widgets }.with_tooltip("Algorithm used to generate the image during each sampling step")
// 	};

// 	let text_guidance = {
// 		let widgets = number_widget(document_node, node_id, text_guidance_index, "Prompt Guidance", NumberInput::default().min(0.).max(30.), true);
// 		LayoutGroup::Row { widgets }.with_tooltip(
// 			"Amplification of the text prompt's influence over the outcome. At 0, the prompt is entirely ignored.\n\
// 			\n\
// 			Lower values are more creative and exploratory. Higher values are more literal and uninspired.\n\
// 			\n\
// 			This parameter is otherwise known as CFG (classifier-free guidance).",
// 		)
// 	};

// 	let text_prompt = {
// 		let widgets = text_area_widget(document_node, node_id, text_index, "Prompt", true);
// 		LayoutGroup::Row { widgets }.with_tooltip(
// 			"Description of the desired image subject and style.\n\
// 			\n\
// 			Include an artist name like \"Rembrandt\" or art medium like \"watercolor\" or \"photography\" to influence the look. List multiple to meld styles.\n\
// 			\n\
// 			To boost (or lessen) the importance of a word or phrase, wrap it in parentheses ending with a colon and a multiplier, for example:\n\
// 			\"Colorless green ideas (sleep:1.3) furiously\"",
// 		)
// 	};
// 	let negative_prompt = {
// 		let widgets = text_area_widget(document_node, node_id, neg_index, "Negative Prompt", true);
// 		LayoutGroup::Row { widgets }.with_tooltip("A negative text prompt can be used to list things like objects or colors to avoid")
// 	};
// 	let base_image = {
// 		let widgets = bool_widget(document_node, node_id, base_img_index, "Adapt Input Image", CheckboxInput::default(), true);
// 		LayoutGroup::Row { widgets }.with_tooltip("Generate an image based upon the bitmap data plugged into this node")
// 	};
// 	let image_creativity = {
// 		let props = NumberInput::default().percentage().disabled(!use_base_image);
// 		let widgets = number_widget(document_node, node_id, img_creativity_index, "Image Creativity", props, true);
// 		LayoutGroup::Row { widgets }.with_tooltip(
// 			"Strength of the artistic liberties allowing changes from the input image. The image is unchanged at 0% and completely different at 100%.\n\
// 			\n\
// 			This parameter is otherwise known as denoising strength.",
// 		)
// 	};

// 	let mut layout = vec![
// 		server_status,
// 		progress,
// 		image_controls,
// 		seed,
// 		resolution,
// 		sampling_steps,
// 		sampling_method,
// 		text_guidance,
// 		text_prompt,
// 		negative_prompt,
// 		base_image,
// 		image_creativity,
// 		// layer_mask,
// 	];

// 	// if use_base_image && layer_reference_input_layer_is_some {
// 	// 	let in_paint = {
// 	// 		let mut widgets = start_widgets(document_node, node_id, inpaint_index, "Inpaint", FrontendGraphDataType::Boolean, true);

// 	// 		if let Some(& TaggedValue::Bool(in_paint)
// 	//)/ 		} = &document_node.inputs[inpaint_index].as_non_exposed_value()
// 	// 		{
// 	// 			widgets.extend_from_slice(&[
// 	// 				Separator::new(SeparatorType::Unrelated).widget_holder(),
// 	// 				RadioInput::new(
// 	// 					[(true, "Inpaint"), (false, "Outpaint")]
// 	// 						.into_iter()
// 	// 						.map(|(paint, name)| RadioEntryData::new(name).label(name).on_update(update_value(move |_| TaggedValue::Bool(paint), node_id, inpaint_index)))
// 	// 						.collect(),
// 	// 				)
// 	// 				.selected_index(Some(1 - in_paint as u32))
// 	// 				.widget_holder(),
// 	// 			]);
// 	// 		}
// 	// 		LayoutGroup::Row { widgets }.with_tooltip(
// 	// 			"Constrain image generation to the interior (inpaint) or exterior (outpaint) of the mask, while referencing the other unchanged parts as context imagery.\n\
// 	// 			\n\
// 	// 			An unwanted part of an image can be replaced by drawing around it with a black shape and inpainting with that mask layer.\n\
// 	// 			\n\
// 	// 			An image can be uncropped by resizing the Imaginate layer to the target bounds and outpainting with a black rectangle mask matching the original image bounds.",
// 	// 		)
// 	// 	};

// 	// 	let blur_radius = {
// 	// 		let number_props = NumberInput::default().unit(" px").min(0.).max(25.).int();
// 	// 		let widgets = number_widget(document_node, node_id, mask_blur_index, "Mask Blur", number_props, true);
// 	// 		LayoutGroup::Row { widgets }.with_tooltip("Blur radius for the mask. Useful for softening sharp edges to blend the masked area with the rest of the image.")
// 	// 	};

// 	// 	let mask_starting_fill = {
// 	// 		let mut widgets = start_widgets(document_node, node_id, mask_fill_index, "Mask Starting Fill", FrontendGraphDataType::General, true);

// 	// 		if let Some(& TaggedValue::ImaginateMaskStartingFill(starting_fill)
// 	//)/ 		} = &document_node.inputs[mask_fill_index].as_non_exposed_value()
// 	// 		{
// 	// 			let mask_fill_content_modes = ImaginateMaskStartingFill::list();
// 	// 			let mut entries = Vec::with_capacity(mask_fill_content_modes.len());
// 	// 			for mode in mask_fill_content_modes {
// 	// 				entries.push(MenuListEntry::new(format!("{mode:?}")).label(mode.to_string()).on_update(update_value(move |_| TaggedValue::ImaginateMaskStartingFill(mode), node_id, mask_fill_index)));
// 	// 			}
// 	// 			let entries = vec![entries];

// 	// 			widgets.extend_from_slice(&[
// 	// 				Separator::new(SeparatorType::Unrelated).widget_holder(),
// 	// 				DropdownInput::new(entries).selected_index(Some(starting_fill as u32)).widget_holder(),
// 	// 			]);
// 	// 		}
// 	// 		LayoutGroup::Row { widgets }.with_tooltip(
// 	// 			"Begin in/outpainting the masked areas using this fill content as the starting input image.\n\
// 	// 			\n\
// 	// 			Each option can be visualized by generating with 'Sampling Steps' set to 0.",
// 	// 		)
// 	// 	};
// 	// 	layout.extend_from_slice(&[in_paint, blur_radius, mask_starting_fill]);
// 	// }

// 	let improve_faces = {
// 		let widgets = bool_widget(document_node, node_id, faces_index, "Improve Faces", CheckboxInput::default(), true);
// 		LayoutGroup::Row { widgets }.with_tooltip(
// 			"Postprocess human (or human-like) faces to look subtly less distorted.\n\
// 			\n\
// 			This filter can be used on its own by enabling 'Adapt Input Image' and setting 'Sampling Steps' to 0.",
// 		)
// 	};
// 	let tiling = {
// 		let widgets = bool_widget(document_node, node_id, tiling_index, "Tiling", CheckboxInput::default(), true);
// 		LayoutGroup::Row { widgets }.with_tooltip("Generate the image so its edges loop seamlessly to make repeatable patterns or textures")
// 	};
// 	layout.extend_from_slice(&[improve_faces, tiling]);

// 	layout
// }

pub(crate) fn node_no_properties(node_id: NodeId, context: &mut NodePropertiesContext) -> Vec<LayoutGroup> {
	let text = if context.network_interface.is_layer(&node_id, context.selection_network_path) {
		"Layer has no properties"
	} else {
		"Node has no properties"
	};
	string_properties(text)
}

pub(crate) fn generate_node_properties(node_id: NodeId, context: &mut NodePropertiesContext) -> LayoutGroup {
	let mut layout = Vec::new();

	if let Some(properties_override) = context
		.network_interface
		.reference(&node_id, context.selection_network_path)
		.cloned()
		.unwrap_or_default()
		.as_ref()
		.and_then(|reference| super::document_node_definitions::resolve_document_node_type(reference))
		.and_then(|definition| definition.properties)
		.and_then(|properties| NODE_OVERRIDES.get(properties))
	{
		layout = properties_override(node_id, context);
	} else {
		let number_of_inputs = context.network_interface.number_of_inputs(&node_id, context.selection_network_path);
		for input_index in 1..number_of_inputs {
			let row = context.call_widget_override(&node_id, input_index).unwrap_or_else(|| {
				let Some(implementation) = context.network_interface.implementation(&node_id, context.selection_network_path) else {
					log::error!("Could not get implementation for node {node_id}");
					return Vec::new();
				};

				let mut number_options = (None, None, None);
				let input_type = match implementation {
					DocumentNodeImplementation::ProtoNode(proto_node_identifier) => 'early_return: {
						if let Some(field) = graphene_core::registry::NODE_METADATA
							.lock()
							.unwrap()
							.get(&proto_node_identifier.name.clone().into_owned())
							.and_then(|metadata| metadata.fields.get(input_index))
						{
							number_options = (field.number_min, field.number_max, field.number_mode_range);
							if let Some(ref default) = field.default_type {
								break 'early_return default.clone();
							}
						}

						let Some(implementations) = &interpreted_executor::node_registry::NODE_REGISTRY.get(proto_node_identifier) else {
							log::error!("Could not get implementation for protonode {proto_node_identifier:?}");
							return Vec::new();
						};

						let proto_node_identifier = proto_node_identifier.clone();

						let mut input_types = implementations
							.keys()
							.filter_map(|item| item.inputs.get(input_index))
							.filter(|ty| property_from_type(node_id, input_index, ty, number_options, context).is_ok())
							.collect::<Vec<_>>();
						input_types.sort_by_key(|ty| ty.type_name());
						let input_type = input_types.first().cloned();

						let Some(input_type) = input_type else {
							log::error!("Could not get input type for protonode {proto_node_identifier:?} at index {input_index:?}");
							return Vec::new();
						};

						input_type.clone()
					}
					_ => context.network_interface.input_type(&InputConnector::node(node_id, input_index), context.selection_network_path).0,
				};

				property_from_type(node_id, input_index, &input_type, number_options, context).unwrap_or_else(|value| value)
			});

			layout.extend(row);
		}
	}

	if layout.is_empty() {
		layout = node_no_properties(node_id, context);
	}
	let name = context
		.network_interface
		.reference(&node_id, context.selection_network_path)
		.cloned()
		.unwrap_or_default() // If there is an error getting the reference, default to empty string
		.or_else(|| {
			// If there is no reference, try to get the proto node name
			context.network_interface.implementation(&node_id, context.selection_network_path).and_then(|implementation|{
				if let DocumentNodeImplementation::ProtoNode(protonode) = implementation {
					Some(protonode.name.clone().into_owned())
				} else {
					None
				}
			})
		})
		.unwrap_or("Custom Node".to_string());
	let description = context.network_interface.description(&node_id, context.selection_network_path);
	let visible = context.network_interface.is_visible(&node_id, context.selection_network_path);
	let pinned = context.network_interface.is_pinned(&node_id, context.selection_network_path);
	LayoutGroup::Section {
		name,
		description,
		visible,
		pinned,
		id: node_id.0,
		layout,
	}
}

/// Fill Node Widgets LayoutGroup
pub(crate) fn fill_properties(node_id: NodeId, context: &mut NodePropertiesContext) -> Vec<LayoutGroup> {
	let document_node = match get_document_node(node_id, context) {
		Ok(document_node) => document_node,
		Err(err) => {
			log::error!("Could not get document node in fill_properties: {err}");
			return Vec::new();
		}
	};
	let fill_index = 1;
	let backup_color_index = 2;
	let backup_gradient_index = 3;

	let mut widgets_first_row = start_widgets(document_node, node_id, fill_index, "Fill", "TODO", FrontendGraphDataType::General, true);

	let (fill, backup_color, backup_gradient) = if let (Some(TaggedValue::Fill(fill)), &Some(&TaggedValue::OptionalColor(backup_color)), Some(TaggedValue::Gradient(backup_gradient))) = (
		&document_node.inputs[fill_index].as_value(),
		&document_node.inputs[backup_color_index].as_value(),
		&document_node.inputs[backup_gradient_index].as_value(),
	) {
		(fill, backup_color, backup_gradient)
	} else {
		return vec![LayoutGroup::Row { widgets: widgets_first_row }];
	};
	let fill2 = fill.clone();
	let backup_color_fill: Fill = backup_color.into();
	let backup_gradient_fill: Fill = backup_gradient.clone().into();

	widgets_first_row.push(Separator::new(SeparatorType::Unrelated).widget_holder());
	widgets_first_row.push(
		ColorInput::default()
			.value(fill.clone().into())
			.on_update(move |x: &ColorInput| {
				Message::Batched(Box::new([
					match &fill2 {
						Fill::None => NodeGraphMessage::SetInputValue {
							node_id,
							input_index: backup_color_index,
							value: TaggedValue::OptionalColor(None),
						}
						.into(),
						Fill::Solid(color) => NodeGraphMessage::SetInputValue {
							node_id,
							input_index: backup_color_index,
							value: TaggedValue::OptionalColor(Some(*color)),
						}
						.into(),
						Fill::Gradient(gradient) => NodeGraphMessage::SetInputValue {
							node_id,
							input_index: backup_gradient_index,
							value: TaggedValue::Gradient(gradient.clone()),
						}
						.into(),
					},
					NodeGraphMessage::SetInputValue {
						node_id,
						input_index: fill_index,
						value: TaggedValue::Fill(x.value.to_fill(fill2.as_gradient())),
					}
					.into(),
				]))
			})
			.on_commit(commit_value)
			.widget_holder(),
	);
	let mut widgets = vec![LayoutGroup::Row { widgets: widgets_first_row }];

	let fill_type_switch = {
		let mut row = vec![TextLabel::new("").widget_holder()];
		match fill {
			Fill::Solid(_) | Fill::None => add_blank_assist(&mut row),
			Fill::Gradient(gradient) => {
				let reverse_button = IconButton::new("Reverse", 24)
					.tooltip("Reverse the gradient color stops")
					.on_update(update_value(
						{
							let gradient = gradient.clone();
							move |_| {
								let mut gradient = gradient.clone();
								gradient.stops = gradient.stops.reversed();
								TaggedValue::Fill(Fill::Gradient(gradient))
							}
						},
						node_id,
						fill_index,
					))
					.widget_holder();
				row.push(Separator::new(SeparatorType::Unrelated).widget_holder());
				row.push(reverse_button);
			}
		}

		let entries = vec![
			RadioEntryData::new("solid")
				.label("Solid")
				.on_update(update_value(move |_| TaggedValue::Fill(backup_color_fill.clone()), node_id, fill_index))
				.on_commit(commit_value),
			RadioEntryData::new("gradient")
				.label("Gradient")
				.on_update(update_value(move |_| TaggedValue::Fill(backup_gradient_fill.clone()), node_id, fill_index))
				.on_commit(commit_value),
		];

		row.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			RadioInput::new(entries).selected_index(Some(if fill.as_gradient().is_some() { 1 } else { 0 })).widget_holder(),
		]);

		LayoutGroup::Row { widgets: row }
	};
	widgets.push(fill_type_switch);

	if let Fill::Gradient(gradient) = fill.clone() {
		let mut row = vec![TextLabel::new("").widget_holder()];
		match gradient.gradient_type {
			GradientType::Linear => add_blank_assist(&mut row),
			GradientType::Radial => {
				let orientation = if (gradient.end.x - gradient.start.x).abs() > f64::EPSILON * 1e6 {
					gradient.end.x > gradient.start.x
				} else {
					(gradient.start.x + gradient.start.y) < (gradient.end.x + gradient.end.y)
				};
				let reverse_radial_gradient_button = IconButton::new(if orientation { "ReverseRadialGradientToRight" } else { "ReverseRadialGradientToLeft" }, 24)
					.tooltip("Reverse which end the gradient radiates from")
					.on_update(update_value(
						{
							let gradient = gradient.clone();
							move |_| {
								let mut gradient = gradient.clone();
								std::mem::swap(&mut gradient.start, &mut gradient.end);
								TaggedValue::Fill(Fill::Gradient(gradient))
							}
						},
						node_id,
						fill_index,
					))
					.widget_holder();
				row.push(Separator::new(SeparatorType::Unrelated).widget_holder());
				row.push(reverse_radial_gradient_button);
			}
		}

		let new_gradient1 = gradient.clone();
		let new_gradient2 = gradient.clone();

		let entries = vec![
			RadioEntryData::new("linear")
				.label("Linear")
				.on_update(update_value(
					move |_| {
						let mut new_gradient = new_gradient1.clone();
						new_gradient.gradient_type = GradientType::Linear;
						TaggedValue::Fill(Fill::Gradient(new_gradient))
					},
					node_id,
					fill_index,
				))
				.on_commit(commit_value),
			RadioEntryData::new("radial")
				.label("Radial")
				.on_update(update_value(
					move |_| {
						let mut new_gradient = new_gradient2.clone();
						new_gradient.gradient_type = GradientType::Radial;
						TaggedValue::Fill(Fill::Gradient(new_gradient))
					},
					node_id,
					fill_index,
				))
				.on_commit(commit_value),
		];

		row.extend_from_slice(&[
			Separator::new(SeparatorType::Unrelated).widget_holder(),
			RadioInput::new(entries).selected_index(Some(gradient.gradient_type as u32)).widget_holder(),
		]);

		widgets.push(LayoutGroup::Row { widgets: row });
	}

	widgets
}

pub fn stroke_properties(node_id: NodeId, context: &mut NodePropertiesContext) -> Vec<LayoutGroup> {
	let document_node = match get_document_node(node_id, context) {
		Ok(document_node) => document_node,
		Err(err) => {
			log::error!("Could not get document node in fill_properties: {err}");
			return Vec::new();
		}
	};
	let color_index = 1;
	let weight_index = 2;
	let dash_lengths_index = 3;
	let dash_offset_index = 4;
	let line_cap_index = 5;
	let line_join_index = 6;
	let miter_limit_index = 7;

	let color = color_widget(document_node, node_id, color_index, "Color", "TODO", ColorInput::default(), true);
	let weight = number_widget(document_node, node_id, weight_index, "Weight", "TODO", NumberInput::default().unit(" px").min(0.), true);

	let dash_lengths_val = match &document_node.inputs[dash_lengths_index].as_value() {
		Some(TaggedValue::VecF64(x)) => x,
		_ => &vec![],
	};
	let dash_lengths = vec_f64_input(document_node, node_id, dash_lengths_index, "Dash Lengths", "TODO", TextInput::default().centered(true), true);
	let number_input = NumberInput::default().unit(" px").disabled(dash_lengths_val.is_empty());
	let dash_offset = number_widget(document_node, node_id, dash_offset_index, "Dash Offset", "TODO", number_input, true);
	let line_cap = line_cap_widget(document_node, node_id, line_cap_index, "Line Cap", "TODO", true);
	let line_join = line_join_widget(document_node, node_id, line_join_index, "Line Join", "TODO", true);
	let line_join_val = match &document_node.inputs[line_join_index].as_value() {
		Some(TaggedValue::LineJoin(x)) => x,
		_ => &LineJoin::Miter,
	};
	let number_input = NumberInput::default().min(0.).disabled(line_join_val != &LineJoin::Miter);
	let miter_limit = number_widget(document_node, node_id, miter_limit_index, "Miter Limit", "TODO", number_input, true);

	vec![
		color,
		LayoutGroup::Row { widgets: weight },
		LayoutGroup::Row { widgets: dash_lengths },
		LayoutGroup::Row { widgets: dash_offset },
		line_cap,
		line_join,
		LayoutGroup::Row { widgets: miter_limit },
	]
}

pub fn offset_path_properties(node_id: NodeId, context: &mut NodePropertiesContext) -> Vec<LayoutGroup> {
	let document_node = match get_document_node(node_id, context) {
		Ok(document_node) => document_node,
		Err(err) => {
			log::error!("Could not get document node in offset_path_properties: {err}");
			return Vec::new();
		}
	};
	let distance_index = 1;
	let line_join_index = 2;
	let miter_limit_index = 3;

	let number_input = NumberInput::default().unit(" px");
	let distance = number_widget(document_node, node_id, distance_index, "Offset", "TODO", number_input, true);

	let line_join = line_join_widget(document_node, node_id, line_join_index, "Line Join", "TODO", true);
	let line_join_val = match &document_node.inputs[line_join_index].as_value() {
		Some(TaggedValue::LineJoin(x)) => x,
		_ => &LineJoin::Miter,
	};

	let number_input = NumberInput::default().min(0.).disabled(line_join_val != &LineJoin::Miter);
	let miter_limit = number_widget(document_node, node_id, miter_limit_index, "Miter Limit", "TODO", number_input, true);

	vec![LayoutGroup::Row { widgets: distance }, line_join, LayoutGroup::Row { widgets: miter_limit }]
}

pub fn math_properties(node_id: NodeId, context: &mut NodePropertiesContext) -> Vec<LayoutGroup> {
	let document_node = match get_document_node(node_id, context) {
		Ok(document_node) => document_node,
		Err(err) => {
			log::error!("Could not get document node in offset_path_properties: {err}");
			return Vec::new();
		}
	};

	let expression_index = 1;
	let operation_b_index = 2;

	let expression = (|| {
		let mut widgets = start_widgets(document_node, node_id, expression_index, "Expression", "TODO", FrontendGraphDataType::General, true);

		let Some(input) = document_node.inputs.get(expression_index) else {
			log::warn!("A widget failed to be built because its node's input index is invalid.");
			return vec![];
		};
		if let Some(TaggedValue::String(x)) = &input.as_non_exposed_value() {
			widgets.extend_from_slice(&[
				Separator::new(SeparatorType::Unrelated).widget_holder(),
				TextInput::new(x.clone())
					.centered(true)
					.on_update(update_value(
						|x: &TextInput| {
							TaggedValue::String({
								let mut expression = x.value.trim().to_string();

								if ["+", "-", "*", "/", "^", "%"].iter().any(|&infix| infix == expression) {
									expression = format!("A {} B", expression);
								} else if expression == "^" {
									expression = String::from("A^B");
								}

								expression
							})
						},
						node_id,
						expression_index,
					))
					.on_commit(commit_value)
					.widget_holder(),
			])
		}
		widgets
	})();
	let operand_b = number_widget(document_node, node_id, operation_b_index, "Operand B", "TODO", NumberInput::default(), true);
	let operand_a_hint = vec![TextLabel::new("(Operand A is the primary input)").widget_holder()];

	vec![
		LayoutGroup::Row { widgets: expression }.with_tooltip(r#"A math expression that may incorporate "A" and/or "B", such as "sqrt(A + B) - B^2""#),
		LayoutGroup::Row { widgets: operand_b }.with_tooltip(r#"The value of "B" when calculating the expression"#),
		LayoutGroup::Row { widgets: operand_a_hint }.with_tooltip(r#""A" is fed by the value from the previous node in the primary data flow, or it is 0 if disconnected"#),
	]
}
