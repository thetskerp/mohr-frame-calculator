//! Небольшой повторяемый замер ядра МКЭ для рам разного размера.

use std::time::Instant;

use mohr_frame::{
    fem::{FemProject, SupportKind},
    Node, Point, Rod, StructuralModel,
};

fn cantilever(members: usize) -> (StructuralModel, FemProject) {
    let mut model = StructuralModel::default();
    for index in 0..=members {
        model.nodes.push(Node {
            id: format!("P{index}"),
            position: Point {
                x: index as f64,
                y: 0.0,
            },
        });
        if index > 0 {
            model
                .rods
                .push(Rod::blank(format!("S{index}"), index - 1, index));
        }
    }
    model.target_node = members;
    let mut fem = FemProject::default();
    fem.reconcile(&model);
    fem.nodes[0].support = SupportKind::Fixed;
    fem.nodes[members].fy = -1.0;
    (model, fem)
}

fn main() {
    for members in [4, 20, 60] {
        let (model, fem) = cantilever(members);
        let start = Instant::now();
        let result = fem
            .solve(&model)
            .expect("устойчивая консоль должна рассчитываться");
        println!(
            "{:>2} стержней, {:>3} СС: {:>8.2} мс; M у заделки = {:.2}",
            members,
            result.total_dof,
            start.elapsed().as_secs_f64() * 1000.0,
            result.actual.members[0].at(0.0).m,
        );
    }
}
