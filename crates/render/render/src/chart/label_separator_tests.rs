use super::*;
use kition_slide_model::SlideFile;

fn fixture(kind: ChartType) -> (Chart, Series, DataLabels) {
    let c = Chart { kind, categories: vec!["Alpha".into()], ..Default::default() };
    let s = Series { name: "Revenue".into(), values: vec![Some(5.0)], ..Default::default() };
    let l = DataLabels { category: true, percent: true, ..Default::default() };
    (c, s, l)
}

#[test]
fn pie_category_percentage_defaults_to_newline_without_changing_number_format() {
    let (mut c, s, mut l) = fixture(ChartType::Pie);
    assert_eq!(label_text(&c, &s, &l, 0, 5.0, Some(0.25)), "Alpha\n25%");
    l.num_fmt = Some("0.0%".into());
    assert_eq!(label_text(&c, &s, &l, 0, 5.0, Some(0.125)), "Alpha\n12.5%");
    c.fmt.three_d = true;
    assert_eq!(label_text(&c, &s, &l, 0, 5.0, Some(0.125)), "Alpha\n12.5%");
    c.fmt.three_d = false;
    c.fmt.of_pie = Some("pie".into());
    assert_eq!(label_text(&c, &s, &l, 0, 5.0, Some(0.125)), "Alpha\n12.5%");
}

#[test]
fn explicit_separators_including_empty_override_pie_default() {
    let (c, s, mut l) = fixture(ChartType::Pie);
    for separator in ["", ", ", "; ", "\n", " | "] {
        l.separator = Some(separator.into());
        assert_eq!(label_text(&c, &s, &l, 0, 5.0, Some(0.25)), format!("Alpha{separator}25%"));
    }
}

#[test]
fn separator_uses_effective_series_type_and_excludes_doughnut() {
    for kind in [ChartType::Doughnut, ChartType::Column, ChartType::Scatter, ChartType::Combo] {
        let (c, s, l) = fixture(kind);
        assert_eq!(label_text(&c, &s, &l, 0, 5.0, Some(0.25)), "Alpha, 25%");
    }
    let (mut c, mut s, l) = fixture(ChartType::Pie);
    s.kind = Some(ChartType::Column);
    assert_eq!(label_text(&c, &s, &l, 0, 5.0, Some(0.25)), "Alpha, 25%");
    c.kind = ChartType::Combo;
    s.kind = Some(ChartType::Pie);
    assert_eq!(label_text(&c, &s, &l, 0, 5.0, Some(0.25)), "Alpha\n25%");
    s.kind = Some(ChartType::Doughnut);
    assert_eq!(label_text(&c, &s, &l, 0, 5.0, Some(0.25)), "Alpha, 25%");
}

#[test]
fn default_exception_uses_enabled_text_fields_not_emitted_part_count() {
    let (c, mut s, l) = fixture(ChartType::Pie);
    let with_value = DataLabels { value: true, ..l.clone() };
    assert_eq!(label_text(&c, &s, &with_value, 0, 5.0, Some(0.25)), "Alpha, 5, 25%");
    let with_series = DataLabels { series: true, ..l.clone() };
    assert_eq!(label_text(&c, &s, &with_series, 0, 5.0, Some(0.25)), "Revenue, Alpha, 25%");
    let with_bubble = DataLabels { bubble_size: true, ..l.clone() };
    assert_eq!(label_text(&c, &s, &with_bubble, 0, 5.0, Some(0.25)), "Alpha, 25%", "a missing bubble value does not change enabled fields");
    s.sizes = vec![Some(2.0)];
    assert_eq!(label_text(&c, &s, &with_bubble, 0, 5.0, Some(0.25)), "Alpha, 25%, 2");
    let category_value = DataLabels { value: true, percent: false, ..l.clone() };
    assert_eq!(label_text(&c, &s, &category_value, 0, 5.0, None), "Alpha, 5");
    let with_key = DataLabels { legend_key: true, ..l.clone() };
    assert_eq!(label_text(&c, &s, &with_key, 0, 5.0, Some(0.25)), "Alpha\n25%");
    assert_eq!(label_text(&c, &s, &l, 0, 5.0, None), "Alpha", "missing percentage data must not append a blank line");
}

#[test]
fn point_labels_use_their_own_effective_flags_and_explicit_separator() {
    let (c, s, mut l) = fixture(ChartType::Pie);
    l.points =
        vec![(0, DataLabels { category: true, value: true, ..Default::default() }), (1, DataLabels { separator: Some(String::new()), ..l.clone() })];
    assert_eq!(label_text(&c, &s, l.point(0).unwrap(), 0, 5.0, Some(0.25)), "Alpha, 5");
    assert_eq!(label_text(&c, &s, l.point(1).unwrap(), 0, 5.0, Some(0.25)), "Alpha25%");
    l.deleted.push(0);
    assert!(l.point(0).is_none());
}

#[test]
fn generated_newline_uses_existing_multiline_label_measurement() {
    let (c, s, mut l) = fixture(ChartType::Pie);
    let multiline = label_text(&c, &s, &l, 0, 5.0, Some(0.25));
    l.separator = Some(", ".into());
    let single = label_text(&c, &s, &l, 0, 5.0, Some(0.25));
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
    let e = Env { rctx: &rctx, c: &c, m: Affine::IDENTITY, pal: Pal { rctx: &rctx, scheme: None }, base, size: (400.0, 200.0) };
    let multi_size = measure(&e, &multiline, &e.base, None);
    let single_size = measure(&e, &single, &e.base, None);
    assert!(multi_size.0 < single_size.0);
    assert!(multi_size.1 > single_size.1);
    let b = body(&multiline, &e.base, Align::Center, false);
    assert_eq!(b.paragraphs.len(), 2);
}
