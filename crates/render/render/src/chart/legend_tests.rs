use super::*;
use kition_slide_model::SlideFile;

fn with_env<T>(c: &Chart, f: impl FnOnce(&Env<'_>) -> T) -> T {
    let p = SlideFile::default();
    let rctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
    let base = Txt {
        font: Some("Inter".into()),
        font_ea: None,
        font_cs: None,
        size: 16.0,
        spacing: None,
        bold: false,
        italic: false,
        color: Rgba::BLACK,
        rot: None,
    };
    f(&Env { rctx: &rctx, c, m: Affine::IDENTITY, pal: Pal { rctx: &rctx, scheme: None }, base, size: (400.0, 200.0) })
}

fn order(c: &Chart) -> Vec<usize> {
    with_env(c, |e| legend_entries(e).into_iter().filter_map(|entry| entry.series).collect())
}

#[test]
fn stacked_legend_order_depends_on_stack_direction_and_legend_position() {
    for kind in [ChartType::StackedColumn, ChartType::PercentColumn, ChartType::StackedLine, ChartType::StackedArea] {
        let mut c = Chart::sample(kind);
        c.legend = Some("r".into());
        assert_eq!(order(&c), [2, 1, 0]);
        for pos in ["t", "b", "l", "tr"] {
            c.legend = Some(pos.into());
            assert_eq!(order(&c), [0, 1, 2], "top/bottom and unverified side positions keep source order");
        }
    }
    for kind in [ChartType::StackedBar, ChartType::PercentBar] {
        let mut c = Chart::sample(kind);
        for pos in ["r", "t", "b"] {
            c.legend = Some(pos.into());
            assert_eq!(order(&c), [0, 1, 2]);
        }
    }
    for kind in [ChartType::Column, ChartType::Line, ChartType::Area] {
        let mut c = Chart::sample(kind);
        c.legend = Some("r".into());
        assert_eq!(order(&c), [0, 1, 2]);
    }
    let mut c = Chart::sample(ChartType::Bar);
    for pos in ["r", "t", "b"] {
        c.legend = Some(pos.into());
        assert_eq!(order(&c), [2, 1, 0], "clustered bars retain their existing order");
    }
}

#[test]
fn reordered_and_deleted_legend_entries_keep_their_series_paint_and_names() {
    for kind in [ChartType::StackedColumn, ChartType::StackedBar] {
        let mut c = Chart::sample(kind);
        c.legend = Some("r".into());
        let colors = [Rgba::rgb(240, 20, 20), Rgba::rgb(20, 180, 20), Rgba::rgb(20, 20, 240)];
        for (i, s) in c.series.iter_mut().enumerate() {
            s.name = format!("Series {i}");
            s.fill = Some(Fill::solid(ColorRef::rgb(colors[i])));
            s.line = Some(Line { fill: Some(Fill::None), ..Default::default() });
        }
        c.fmt.legend_deleted.push(1);
        let original = c.clone();
        with_env(&c, |e| {
            let entries = legend_entries(e);
            let expected = if kind == ChartType::StackedColumn { [2, 0] } else { [0, 2] };
            assert_eq!(entries.iter().filter_map(|entry| entry.series).collect::<Vec<_>>(), expected);
            let layout = legend_layout(e, &entries, &e.base, "r", Rect::new(0.0, 0.0, 400.0, 200.0), 400.0, 200.0);
            let mut ctx = RenderContext::new_with(400, 200, vello_cpu::RenderSettings { num_threads: 0, ..Default::default() });
            ctx.set_paint(color(Rgba::WHITE, 1.0));
            ctx.fill_rect(&Rect::new(0.0, 0.0, 400.0, 200.0));
            draw_legend(&mut ctx, e, &entries, &layout, &e.base);
            let image = crate::Renderer::default().finish(ctx, 400, 200);
            for ((entry, (key, _)), i) in entries.iter().zip(&layout.items).zip(expected) {
                assert_eq!(entry.text, c.series[i].name);
                assert_eq!(entry.fill, c.series[i].fill);
                let point = key.center();
                assert_eq!(image.pixel(point.x as u32, point.y as u32), [colors[i].r, colors[i].g, colors[i].b, 255]);
            }
        });
        assert_eq!(c, original, "legend layout must not reorder the chart's data");
    }
}

#[test]
fn mixed_groups_and_reversed_value_axes_keep_existing_order() {
    let mut c = Chart::sample(ChartType::StackedColumn);
    c.legend = Some("r".into());
    c.series[1].kind = Some(ChartType::Line);
    assert_eq!(order(&c), [0, 1, 2]);
    c.series[1].kind = None;
    c.fmt.axes = vec![
        Axis { id: 1, kind: AxisKind::Category, pos: "b".into(), cross_ax: 2, ..Default::default() },
        Axis { id: 2, kind: AxisKind::Value, pos: "l".into(), reversed: true, cross_ax: 1, ..Default::default() },
    ];
    for s in &mut c.series {
        s.fmt.axes = vec![1, 2];
    }
    assert_eq!(order(&c), [0, 1, 2]);
    c.fmt.axes[1].reversed = false;
    c.fmt.axes.push(Axis { id: 3, kind: AxisKind::Value, pos: "r".into(), cross_ax: 1, ..Default::default() });
    c.series[1].fmt.axes = vec![1, 3];
    assert_eq!(order(&c), [0, 1, 2]);
}
