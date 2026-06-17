use std::path::PathBuf;

use egui_kittest::Harness;
use jellyflow_egui::{JellyflowEguiApp, SampleGraphKind, egui};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/jellyflow-egui-gallery"));
    std::fs::create_dir_all(&output_dir)?;

    for sample in SampleGraphKind::ALL {
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(1280.0, 780.0))
            .with_theme(egui::Theme::Light)
            .wgpu()
            .build_eframe(|_cc| {
                JellyflowEguiApp::sample(sample).unwrap_or_else(|err| {
                    panic!("failed to build {} sample: {err}", sample.label())
                })
            });

        harness.run_steps(2);
        let image = harness.render().map_err(std::io::Error::other)?;
        let path = output_dir.join(format!("{}.png", sample_file_name(sample)));
        image.save(&path)?;
        println!("Wrote {}", path.display());
    }

    println!("Wrote gallery snapshots to {}", output_dir.display());
    Ok(())
}

fn sample_file_name(sample: SampleGraphKind) -> &'static str {
    match sample {
        SampleGraphKind::Workflow => "workflow",
        SampleGraphKind::AutomationBuilder => "automation-builder",
        SampleGraphKind::MindMap => "mind-map",
        SampleGraphKind::Tree => "tree",
        SampleGraphKind::OrgChart => "org-chart",
        SampleGraphKind::KnowledgeBoard => "knowledge-board",
        SampleGraphKind::Erd => "erd",
    }
}
