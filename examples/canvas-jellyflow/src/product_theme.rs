//! Application-owned visual tokens for the native Jellyflow examples.

use jellyflow_core::TypeDesc;
use jellyflow_open_gpui::chrome::OpenGpuiEdgePainterRegistry;
use jellyflow_open_gpui::theme::{
    OpenGpuiGraphMetrics, OpenGpuiGraphPalette, OpenGpuiGraphStyleRecipe, OpenGpuiGraphStyleSlot,
    OpenGpuiGraphTheme, OpenGpuiPortMarkerShape, OpenGpuiPortMarkerShapes,
};
use open_gpui::{Hsla, rgb, rgba};

/// Bounded style slot used by Float shader ports and wires.
pub const NATIVE_FLOAT_STYLE_SLOT: OpenGpuiGraphStyleSlot = match OpenGpuiGraphStyleSlot::new(0) {
    Some(slot) => slot,
    None => panic!("Float style slot must fit the native style budget"),
};

/// Product vocabulary selected by one native graph surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeProductProfile {
    /// Dense technical chrome inspired by Shader Graph and Blueprint editors.
    ShaderBlueprint,
    /// Quiet operational chrome inspired by Dify workflow editors.
    WorkflowDify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeSurfaceTokens {
    pub app: Hsla,
    pub panel: Hsla,
    pub raised: Hsla,
    pub inset: Hsla,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeCanvasTokens {
    pub surface: Hsla,
    pub guide: Hsla,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeGridTokens {
    pub minor: Hsla,
    pub major: Hsla,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeNodeTokens {
    pub surface: Hsla,
    pub alternate_surface: Hsla,
    pub border: Hsla,
    pub alternate_border: Hsla,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeHeaderTokens {
    pub surface: Hsla,
    pub alternate_surface: Hsla,
    pub border: Hsla,
    pub metadata: Hsla,
    pub badge_surface: Hsla,
    pub badge_text: Hsla,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeControlTokens {
    pub surface: Hsla,
    pub hover: Hsla,
    pub selected: Hsla,
    pub disabled: Hsla,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeOverlayTokens {
    pub surface: Hsla,
    pub border: Hsla,
    pub diagnostic_surface: Hsla,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeTextTokens {
    pub primary: Hsla,
    pub secondary: Hsla,
    pub muted: Hsla,
    pub inverse: Hsla,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeBorderTokens {
    pub subtle: Hsla,
    pub normal: Hsla,
    pub strong: Hsla,
    pub focus: Hsla,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeToneTokens {
    pub text: Hsla,
    pub surface: Hsla,
    pub border: Hsla,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeStatusTokens {
    pub success: NativeToneTokens,
    pub warning: NativeToneTokens,
    pub conflict: NativeToneTokens,
    pub danger: NativeToneTokens,
    pub info: NativeToneTokens,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeSelectionTokens {
    pub surface: Hsla,
    pub border: Hsla,
    pub focus: Hsla,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativePortTokens {
    pub hovered: Hsla,
    pub source: Hsla,
    pub valid: Hsla,
    pub invalid: Hsla,
    pub disabled: Hsla,
    /// Stable data-type identity shared by Float ports and their wires.
    pub float: Hsla,
    pub shapes: OpenGpuiPortMarkerShapes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeEdgeTokens {
    pub default: Hsla,
    pub data: Hsla,
    pub control: Hsla,
    pub success: Hsla,
    pub warning: Hsla,
    pub danger: Hsla,
    pub hover_tint: Hsla,
    pub selection_tint: Hsla,
    pub active_tint: Hsla,
    pub preview_tint: Hsla,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeSpacingTokens {
    pub xxs: f32,
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub node_gutter: f32,
    pub port_gutter: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeRadiusTokens {
    pub control: f32,
    pub panel: f32,
    pub node: f32,
}

/// Complete product-owned theme shared by the native example shell and node entities.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeProductTheme {
    pub profile: NativeProductProfile,
    pub surface: NativeSurfaceTokens,
    pub canvas: NativeCanvasTokens,
    pub grid: NativeGridTokens,
    pub node: NativeNodeTokens,
    pub header: NativeHeaderTokens,
    pub control: NativeControlTokens,
    pub overlay: NativeOverlayTokens,
    pub text: NativeTextTokens,
    pub border: NativeBorderTokens,
    pub status: NativeStatusTokens,
    pub selection: NativeSelectionTokens,
    pub port: NativePortTokens,
    pub edge: NativeEdgeTokens,
    pub spacing: NativeSpacingTokens,
    pub radius: NativeRadiusTokens,
}

impl NativeProductTheme {
    /// Returns the technical Shader/Blueprint product profile.
    #[must_use]
    pub fn shader_blueprint() -> Self {
        Self {
            profile: NativeProductProfile::ShaderBlueprint,
            surface: NativeSurfaceTokens {
                app: rgb(0x101419).into(),
                panel: rgb(0x151b21).into(),
                raised: rgb(0x1d252d).into(),
                inset: rgb(0x11171d).into(),
            },
            canvas: NativeCanvasTokens {
                surface: rgb(0x0d1217).into(),
                guide: rgb(0x6db7d8).into(),
            },
            grid: NativeGridTokens {
                minor: rgba(0x6b829523).into(),
                major: rgba(0x7d96aa40).into(),
            },
            node: NativeNodeTokens {
                surface: rgb(0x1c242b).into(),
                alternate_surface: rgb(0x172630).into(),
                border: rgb(0x394754).into(),
                alternate_border: rgb(0x426b80).into(),
            },
            header: NativeHeaderTokens {
                surface: rgb(0x232d36).into(),
                alternate_surface: rgb(0x193441).into(),
                border: rgb(0x394754).into(),
                metadata: rgb(0xa3b2bf).into(),
                badge_surface: rgb(0x2c3944).into(),
                badge_text: rgb(0xb6c4cf).into(),
            },
            control: NativeControlTokens {
                surface: rgb(0x171e25).into(),
                hover: rgb(0x26323c).into(),
                selected: rgb(0x2d3f4d).into(),
                disabled: rgba(0x66768770).into(),
            },
            overlay: NativeOverlayTokens {
                surface: rgb(0x182027).into(),
                border: rgb(0x3b4a57).into(),
                diagnostic_surface: rgb(0x352428).into(),
            },
            text: NativeTextTokens {
                primary: rgb(0xf0f4f7).into(),
                secondary: rgb(0xcbd6de).into(),
                muted: rgb(0x91a0ad).into(),
                inverse: rgb(0x101419).into(),
            },
            border: NativeBorderTokens {
                subtle: rgb(0x29343e).into(),
                normal: rgb(0x394754).into(),
                strong: rgb(0x527087).into(),
                focus: rgb(0xa7d7ee).into(),
            },
            status: NativeStatusTokens {
                success: tone(0x88d8b7, 0x1d3a31, 0x315e4e),
                warning: tone(0xe7cc8c, 0x3b3424, 0x6d5d34),
                conflict: tone(0xf2b294, 0x3c2925, 0x865743),
                danger: tone(0xffa0a8, 0x3a2228, 0x84424c),
                info: tone(0xa4d4f0, 0x203542, 0x3b6378),
            },
            selection: NativeSelectionTokens {
                surface: rgb(0x263e4c).into(),
                border: rgb(0x75b9d8).into(),
                focus: rgb(0xa7d7ee).into(),
            },
            port: NativePortTokens {
                hovered: rgb(0xe1e8ed).into(),
                source: rgb(0x79c6e6).into(),
                valid: rgb(0x7dd6ad).into(),
                invalid: rgb(0xff8f96).into(),
                disabled: rgba(0x6879878f).into(),
                float: rgb(0xe1bd67).into(),
                shapes: OpenGpuiPortMarkerShapes {
                    default: OpenGpuiPortMarkerShape::Circle,
                    data: OpenGpuiPortMarkerShape::Diamond,
                    control: OpenGpuiPortMarkerShape::Square,
                },
            },
            edge: NativeEdgeTokens {
                default: rgb(0x8296a5).into(),
                data: rgb(0x5bc1a9).into(),
                control: rgb(0x76b6d3).into(),
                success: rgb(0x7dd6ad).into(),
                warning: rgb(0xe1bd67).into(),
                danger: rgb(0xf08f98).into(),
                hover_tint: rgba(0xffffff2e).into(),
                selection_tint: rgba(0x75b9d8b8).into(),
                active_tint: rgba(0xa7d7eedb).into(),
                preview_tint: rgba(0x79c6e6eb).into(),
            },
            spacing: spacing(),
            radius: radius(),
        }
    }

    /// Returns the operational Workflow/Dify product profile.
    #[must_use]
    pub fn workflow_dify() -> Self {
        Self {
            profile: NativeProductProfile::WorkflowDify,
            surface: NativeSurfaceTokens {
                app: rgb(0x121519).into(),
                panel: rgb(0x181c21).into(),
                raised: rgb(0x20252b).into(),
                inset: rgb(0x14181d).into(),
            },
            canvas: NativeCanvasTokens {
                surface: rgb(0x101318).into(),
                guide: rgb(0x79a9ff).into(),
            },
            grid: NativeGridTokens {
                minor: rgba(0x63708025).into(),
                major: rgba(0x7484993d).into(),
            },
            node: NativeNodeTokens {
                surface: rgb(0x1c2733).into(),
                alternate_surface: rgb(0x25272b).into(),
                border: rgb(0x3d5872).into(),
                alternate_border: rgb(0x625a43).into(),
            },
            header: NativeHeaderTokens {
                surface: rgb(0x222c36).into(),
                alternate_surface: rgb(0x2b2d31).into(),
                border: rgb(0x414a54).into(),
                metadata: rgb(0xaab5c2).into(),
                badge_surface: rgb(0x303944).into(),
                badge_text: rgb(0xb9c4cf).into(),
            },
            control: NativeControlTokens {
                surface: rgb(0x1b2026).into(),
                hover: rgb(0x272f38).into(),
                selected: rgb(0x2c3745).into(),
                disabled: rgba(0x68738170).into(),
            },
            overlay: NativeOverlayTokens {
                surface: rgb(0x1b2026).into(),
                border: rgb(0x3d4652).into(),
                diagnostic_surface: rgb(0x392729).into(),
            },
            text: NativeTextTokens {
                primary: rgb(0xeff3f7).into(),
                secondary: rgb(0xd5dde5).into(),
                muted: rgb(0x929eaa).into(),
                inverse: rgb(0x111419).into(),
            },
            border: NativeBorderTokens {
                subtle: rgb(0x292f36).into(),
                normal: rgb(0x38414b).into(),
                strong: rgb(0x5a6775).into(),
                focus: rgb(0xb8d1f1).into(),
            },
            status: NativeStatusTokens {
                success: tone(0x9ee6b8, 0x234433, 0x376b50),
                warning: tone(0xf0d28a, 0x4a3d25, 0x75613a),
                conflict: tone(0xf0b68f, 0x4d3028, 0x825445),
                danger: tone(0xf19a93, 0x4d292d, 0x84454a),
                info: tone(0x9fc4ff, 0x27364b, 0x48698f),
            },
            selection: NativeSelectionTokens {
                surface: rgb(0x2d3745).into(),
                border: rgb(0x6f94c9).into(),
                focus: rgb(0xb8d1f1).into(),
            },
            port: NativePortTokens {
                hovered: rgb(0xd7dde8).into(),
                source: rgb(0x78a9ff).into(),
                valid: rgb(0x7fd5ac).into(),
                invalid: rgb(0xff8b84).into(),
                disabled: rgba(0x64748b8f).into(),
                float: rgb(0xe0bd63).into(),
                shapes: OpenGpuiPortMarkerShapes {
                    default: OpenGpuiPortMarkerShape::Circle,
                    data: OpenGpuiPortMarkerShape::Circle,
                    control: OpenGpuiPortMarkerShape::Triangle,
                },
            },
            edge: NativeEdgeTokens {
                default: rgb(0x8793a2).into(),
                data: rgb(0x59c2a6).into(),
                control: rgb(0x78a9ff).into(),
                success: rgb(0x7fd5ac).into(),
                warning: rgb(0xe6cf9e).into(),
                danger: rgb(0xe58b7b).into(),
                hover_tint: rgba(0xffffff29).into(),
                selection_tint: rgba(0x78a9ffb3).into(),
                active_tint: rgba(0x9fc4ffd9).into(),
                preview_tint: rgba(0x78a9ffe6).into(),
            },
            spacing: spacing(),
            radius: radius(),
        }
    }

    /// Derives the bounded graph-chrome theme consumed by Jellyflow's Open GPUI host.
    #[must_use]
    pub fn graph_theme(self) -> OpenGpuiGraphTheme {
        let palette = OpenGpuiGraphPalette {
            canvas: self.canvas.surface,
            grid_minor: self.grid.minor,
            grid_major: self.grid.major,
            guide: self.canvas.guide,
            node_surface: self.node.surface,
            node_border: self.node.border,
            selection: self.selection.border,
            focus: self.selection.focus,
            port_hovered: self.port.hovered,
            port_source: self.port.source,
            port_valid: self.port.valid,
            port_invalid: self.port.invalid,
            port_disabled: self.port.disabled,
            edge_default: self.edge.default,
            edge_data: self.edge.data,
            edge_control: self.edge.control,
            edge_success: self.edge.success,
            edge_warning: self.edge.warning,
            edge_danger: self.edge.danger,
            edge_hover_tint: self.edge.hover_tint,
            edge_selection_tint: self.edge.selection_tint,
            edge_active_tint: self.edge.active_tint,
            edge_preview_tint: self.edge.preview_tint,
            label_text: self.text.primary,
            label_background: self.surface.inset,
            diagnostic_warning: self.status.warning.text,
            diagnostic_error: self.status.danger.text,
        };
        let mut metrics = OpenGpuiGraphMetrics {
            node_corner_radius: self.radius.node,
            ..OpenGpuiGraphMetrics::default()
        };
        if self.profile == NativeProductProfile::ShaderBlueprint {
            metrics.port_marker_diameter = 7.0;
            metrics.port_hover_marker_diameter = 9.0;
            metrics.port_layout_diameter = 11.0;
            metrics.edge_ordinary_width = 2.0;
            metrics.edge_hovered_width = 2.5;
            metrics.edge_selected_width = 3.0;
            metrics.edge_active_width = 3.0;
            metrics.edge_strong_ordinary_width = 2.5;
            metrics.edge_strong_hovered_width = 3.0;
            metrics.edge_strong_selected_width = 3.5;
            metrics.edge_strong_active_width = 3.5;
        }
        let theme =
            OpenGpuiGraphTheme::new(palette, metrics).with_port_marker_shapes(self.port.shapes);
        if self.profile == NativeProductProfile::ShaderBlueprint {
            theme.with_style_slot(
                NATIVE_FLOAT_STYLE_SLOT,
                OpenGpuiGraphStyleRecipe::new(self.port.float)
                    .with_port_marker_shape(OpenGpuiPortMarkerShape::Diamond),
            )
        } else {
            theme
        }
    }

    /// Derives application type-to-wire styling for the selected product profile.
    #[must_use]
    pub fn edge_painter_registry(self) -> OpenGpuiEdgePainterRegistry {
        let mut registry = OpenGpuiEdgePainterRegistry::builder();
        if self.profile == NativeProductProfile::ShaderBlueprint {
            registry
                .register_type_style(TypeDesc::Float, NATIVE_FLOAT_STYLE_SLOT)
                .expect("Float style registration must fit the fixed registry budget");
        }
        registry.build()
    }
}

impl Default for NativeProductTheme {
    fn default() -> Self {
        Self::workflow_dify()
    }
}

fn tone(text: u32, surface: u32, border: u32) -> NativeToneTokens {
    NativeToneTokens {
        text: rgb(text).into(),
        surface: rgb(surface).into(),
        border: rgb(border).into(),
    }
}

const fn spacing() -> NativeSpacingTokens {
    NativeSpacingTokens {
        xxs: 2.0,
        xs: 4.0,
        sm: 6.0,
        md: 8.0,
        lg: 10.0,
        xl: 14.0,
        node_gutter: 12.0,
        port_gutter: 6.0,
    }
}

const fn radius() -> NativeRadiusTokens {
    NativeRadiusTokens {
        control: 4.0,
        panel: 6.0,
        node: 6.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_profiles_derive_distinct_valid_graph_themes() {
        let shader = NativeProductTheme::shader_blueprint();
        let workflow = NativeProductTheme::workflow_dify();

        assert!(shader.graph_theme().metrics.is_valid());
        assert!(workflow.graph_theme().metrics.is_valid());
        assert_eq!(shader.graph_theme().metrics.port_marker_diameter, 7.0);
        assert_eq!(shader.graph_theme().metrics.port_hover_marker_diameter, 9.0);
        assert_eq!(shader.graph_theme().metrics.port_layout_diameter, 11.0);
        assert_eq!(shader.graph_theme().metrics.port_hit_diameter, 22.0);
        assert_eq!(workflow.graph_theme().metrics.port_marker_diameter, 8.0);
        assert_eq!(workflow.graph_theme().metrics.port_layout_diameter, 12.0);
        assert_ne!(shader.canvas.surface, workflow.canvas.surface);
        assert_ne!(shader.graph_theme().metrics, workflow.graph_theme().metrics);
        assert!(
            shader
                .graph_theme()
                .style_slot(NATIVE_FLOAT_STYLE_SLOT)
                .is_some()
        );
        assert!(
            workflow
                .graph_theme()
                .style_slot(NATIVE_FLOAT_STYLE_SLOT)
                .is_none()
        );
    }

    #[test]
    fn profiles_publish_semantic_port_shapes() {
        let shader = NativeProductTheme::shader_blueprint();
        let workflow = NativeProductTheme::workflow_dify();

        assert_eq!(shader.port.shapes.default, OpenGpuiPortMarkerShape::Circle);
        assert_eq!(shader.port.shapes.data, OpenGpuiPortMarkerShape::Diamond);
        assert_eq!(shader.port.shapes.control, OpenGpuiPortMarkerShape::Square);
        assert_eq!(
            workflow.port.shapes.default,
            OpenGpuiPortMarkerShape::Circle
        );
        assert_eq!(
            workflow.port.shapes.control,
            OpenGpuiPortMarkerShape::Triangle
        );
    }

    #[test]
    fn interactive_and_status_states_remain_visually_distinct() {
        for theme in [
            NativeProductTheme::shader_blueprint(),
            NativeProductTheme::workflow_dify(),
        ] {
            assert_ne!(theme.control.surface, theme.control.hover);
            assert_ne!(theme.control.hover, theme.selection.surface);
            assert_ne!(theme.selection.border, theme.selection.focus);
            assert_ne!(theme.status.success, theme.status.warning);
            assert_ne!(theme.status.warning, theme.status.conflict);
            assert_ne!(theme.status.conflict, theme.status.danger);
        }
    }
}
