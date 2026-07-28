use super::super::*;

const LOD_RELATIVE_JUMP_TOLERANCE: f32 = 0.04;
const LOD_ABSOLUTE_JUMP_TOLERANCE: f32 = 1.0;
const LOD_ORIGIN_JUMP_TOLERANCE: f32 = 1.0;

impl LodSample {
    const fn zoom(self) -> f32 {
        match self {
            Self::Lod086 => 0.86,
            Self::Lod085 => 0.85,
            Self::Lod084 => 0.84,
        }
    }

    const fn capture_state(self) -> SmokeCaptureState {
        match self {
            Self::Lod086 => SmokeCaptureState::Lod086,
            Self::Lod085 => SmokeCaptureState::Lod085,
            Self::Lod084 => SmokeCaptureState::Lod084,
        }
    }

    const fn next(self) -> Option<Self> {
        match self {
            Self::Lod086 => Some(Self::Lod085),
            Self::Lod085 => Some(Self::Lod084),
            Self::Lod084 => None,
        }
    }
}

impl RealWindowSmokeProbe {
    pub(super) fn advance_lod(
        &mut self,
        window: &mut Window,
        _cx: &mut App,
        _surface: &Entity<NativeGraphSurface>,
        view: &WeakViewSession,
        expectations: &SmokeExpectations,
        surface_size: ViewSize,
    ) -> Result<Option<SmokeInputEvidence>, &'static str> {
        match self.phase {
            ProbePhase::LodSet => {
                let sample = self.lod()?.current;
                let layout_revision_before = view
                    .geometry_snapshot()
                    .map_err(|_| "geometry_snapshot_read_failed")?
                    .layout_revision();
                self.lod_mut()?.layout_revision_before = layout_revision_before;
                set_zoom_preserving_surface_center(view, surface_size, sample.zoom())?;
                window.refresh();
                self.transition(ProbePhase::LodWait);
                Ok(None)
            }
            ProbePhase::LodWait => {
                let lod = self.lod()?;
                let expected = lod.current.zoom();
                let layout_revision_before = lod.layout_revision_before;
                let transform = view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?;
                let layout_revision_after = view
                    .geometry_snapshot()
                    .map_err(|_| "geometry_snapshot_read_failed")?
                    .layout_revision();
                if (transform.zoom - expected).abs() <= TARGET_ZOOM_TOLERANCE
                    && layout_revision_after > layout_revision_before
                {
                    self.transition(ProbePhase::LodCapture);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::LodCapture => {
                let sample = self.lod()?.current;
                let capture_state = sample.capture_state();
                let lod = self.lod()?;
                lod.ensure_capture_pending(capture_state)?;
                if lod.frames.contains_key(&sample) {
                    return Err("input_duplicate_lod_sample");
                }
                let screenshot_path = self.config.capture_path(self.demo, capture_state);
                let (snapshot, transform, geometry, paint, _) = inspect_current_frame(
                    window,
                    view,
                    expectations,
                    surface_size,
                    screenshot_path.as_deref(),
                    None,
                    self.paint_palette,
                )?;
                if let Some(reason) = paint.failure_reason() {
                    return Err(reason);
                }

                let node = match self.lod()?.node {
                    Some(node) => node,
                    None => *geometry
                        .node_regions
                        .keys()
                        .next()
                        .ok_or("input_lod_visible_node_missing")?,
                };
                let frame = lod_frame(node, sample.zoom(), &snapshot, transform, expectations)?;
                let comparison = self
                    .lod()?
                    .frames
                    .last_key_value()
                    .map(|(_, previous)| compare_lod_frames(previous, &frame))
                    .transpose()?;
                let lod = self.lod_mut()?;
                lod.node = Some(node);
                if let Some(comparison) = comparison {
                    lod.max_jump_in_view = lod.max_jump_in_view.max(comparison.max_jump_in_view);
                    lod.continuous &= comparison.continuous;
                }
                if lod.frames.insert(sample, frame).is_some() {
                    return Err("input_duplicate_lod_sample");
                }
                lod.record_capture(capture_state)?;
                self.geometry = Some(geometry);
                self.paint = Some(paint);

                match sample.next() {
                    Some(next) => {
                        self.lod_mut()?.current = next;
                        self.transition(ProbePhase::LodSet);
                    }
                    None => self.transition(ProbePhase::Complete),
                }
                Ok(None)
            }
            _ => unreachable!("LOD scenario received an unrelated phase"),
        }
    }
}

fn lod_frame(
    node: jellyflow_core::NodeId,
    zoom: f32,
    snapshot: &jellyflow_runtime::view::GeometrySnapshot,
    transform: ViewportTransform,
    expectations: &SmokeExpectations,
) -> Result<LodFrameEvidence, &'static str> {
    let geometry = snapshot
        .node(node)
        .filter(|geometry| geometry.is_fresh())
        .ok_or("input_lod_node_geometry_missing")?;
    let origin = transform
        .screen_point_for_canvas(geometry.bounds().origin)
        .ok_or("input_lod_node_projection_failed")?;
    let max = transform
        .screen_point_for_canvas(CanvasPoint {
            x: geometry.bounds().origin.x + geometry.bounds().size.width,
            y: geometry.bounds().origin.y + geometry.bounds().size.height,
        })
        .ok_or("input_lod_node_projection_failed")?;
    let ports = expectations
        .node_ports
        .get(&node)
        .ok_or("input_lod_node_ports_missing")?
        .iter()
        .map(|port| {
            let geometry = snapshot
                .port_outcome(*port)
                .and_then(jellyflow_runtime::view::PublishedPortOutcome::rendered)
                .ok_or("input_lod_port_geometry_missing")?;
            let anchor = transform
                .screen_point_for_canvas(geometry.route_anchor())
                .ok_or("input_lod_port_projection_failed")?;
            Ok((*port, anchor))
        })
        .collect::<Result<BTreeMap<_, _>, &'static str>>()?;
    Ok(LodFrameEvidence {
        zoom,
        node,
        canvas_origin: geometry.bounds().origin,
        origin: ViewPoint::new(origin.x, origin.y),
        size: ViewSize::new(max.x - origin.x, max.y - origin.y),
        ports,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LodComparison {
    continuous: bool,
    max_jump_in_view: f32,
}

fn compare_lod_frames(
    previous: &LodFrameEvidence,
    current: &LodFrameEvidence,
) -> Result<LodComparison, &'static str> {
    if previous.node != current.node || previous.ports.keys().ne(current.ports.keys()) {
        return Err("input_lod_identity_changed");
    }
    if current.zoom >= previous.zoom {
        return Err("input_lod_order_invalid");
    }

    let size_scale = previous
        .size
        .width
        .max(previous.size.height)
        .max(current.size.width)
        .max(current.size.height);
    let geometry_tolerance =
        size_scale.mul_add(LOD_RELATIVE_JUMP_TOLERANCE, LOD_ABSOLUTE_JUMP_TOLERANCE);
    let mut max_jump = 0.0_f32;
    let mut geometry_continuous = true;
    for jump in [
        (previous.size.width - current.size.width).abs(),
        (previous.size.height - current.size.height).abs(),
    ] {
        max_jump = max_jump.max(jump);
        geometry_continuous &= jump <= geometry_tolerance;
    }
    let comparison_zoom = previous.zoom.max(current.zoom);
    for jump in [
        (previous.canvas_origin.x - current.canvas_origin.x).abs() * comparison_zoom,
        (previous.canvas_origin.y - current.canvas_origin.y).abs() * comparison_zoom,
    ] {
        max_jump = max_jump.max(jump);
        geometry_continuous &= jump <= LOD_ORIGIN_JUMP_TOLERANCE;
    }
    for (port, previous_anchor) in &previous.ports {
        let current_anchor = current.ports[port];
        for jump in [
            ((previous_anchor.x - previous.origin.x) - (current_anchor.x - current.origin.x)).abs(),
            ((previous_anchor.y - previous.origin.y) - (current_anchor.y - current.origin.y)).abs(),
        ] {
            max_jump = max_jump.max(jump);
            geometry_continuous &= jump <= geometry_tolerance;
        }
    }

    Ok(LodComparison {
        continuous: geometry_continuous,
        max_jump_in_view: max_jump,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow_core::{NodeId, PortId};

    fn frame(
        zoom: f32,
        canvas_origin: CanvasPoint,
        origin: ViewPoint,
        size: ViewSize,
        port: ViewPoint,
    ) -> LodFrameEvidence {
        LodFrameEvidence {
            zoom,
            node: NodeId::from_u128(1),
            canvas_origin,
            origin,
            size,
            ports: BTreeMap::from([(PortId::from_u128(2), port)]),
        }
    }

    #[test]
    fn lod_continuity_accepts_threshold_scale_and_rejects_presentation_jump() {
        let detail = frame(
            0.86,
            CanvasPoint { x: 20.0, y: 40.0 },
            ViewPoint::new(100.0, 100.0),
            ViewSize::new(220.0, 120.0),
            ViewPoint::new(100.0, 160.0),
        );
        let continuous = frame(
            0.85,
            CanvasPoint { x: 20.0, y: 40.0 },
            ViewPoint::new(104.9, 101.5),
            ViewSize::new(218.0, 119.0),
            ViewPoint::new(104.9, 161.0),
        );
        let jump = frame(
            0.85,
            CanvasPoint { x: 36.0, y: 48.0 },
            ViewPoint::new(116.5, 109.0),
            ViewSize::new(187.0, 102.0),
            ViewPoint::new(116.5, 143.0),
        );

        assert!(
            compare_lod_frames(&detail, &continuous)
                .expect("valid comparison")
                .continuous
        );
        assert!(
            !compare_lod_frames(&detail, &jump)
                .expect("valid comparison")
                .continuous
        );
    }
}
