//! C8-land-9s: integer object ids must remain premises after an SDK update.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::session::model::ReplicaPort;
use kition_kit::{KitDocument, testing::check_id_params};
use kition_slide_kit::SlideDocument;

#[test]
fn every_integer_id_parameter_is_marked() {
    let doc = SlideDocument::create("Ids.pptx", 960, 540).unwrap();
    let commands = doc.commands();
    let allow = [];
    if let Err(problems) = check_id_params(&commands, &allow) {
        panic!("{}", problems.join("\n"));
    }
    let targets: usize = commands.iter().filter(|command| command.targets.is_some()).count();
    let objects: usize = commands.iter().map(|command| command.objects.len()).sum();
    let places: usize = commands.iter().map(|command| command.places.len()).sum();
    let mut unmarked = commands.clone();
    for command in &mut unmarked {
        command.targets = None;
        command.objects = &[];
        command.places = &[];
    }
    let scanned = check_id_params(&unmarked, &[]).err().map_or(0, |problems| problems.len());
    println!("scanner: {scanned} integer id parameters marked; {} allowed (no exceptions)", allow.len());
    println!(
        "{} commands: {targets} target parameters, {objects} object parameters, {places} place parameters; {} allowed",
        commands.len(),
        allow.len()
    );
    for command in &commands {
        for (name, _) in command.object_types {
            if !name.contains('.') {
                assert!(
                    command.schema.as_ref().is_some_and(|schema| schema.get(*name).is_some()),
                    "{}: nonexistent object parameter {name}",
                    command.id
                );
            }
        }
        for name in command.targets.into_iter().chain(command.objects.iter().copied()) {
            assert!(
                command.object_types.iter().any(|(param, types)| *param == name && !types.is_empty()),
                "{} {name}: the object type is declared",
                command.id
            );
        }
    }
}

#[test]
fn environment_labels_name_slides_and_shapes() {
    let doc = SlideDocument::create("Labels.pptx", 960, 540).unwrap();
    let schema = doc.model_schema();
    assert_eq!(schema.type_def("Slide").unwrap().label.as_deref(), Some("props.name"));
    assert_eq!(schema.type_def("Shape").unwrap().label.as_deref(), Some("name"));
}

#[test]
fn environment_does_not_sample_shape_kind_as_a_scalar_edit() {
    let doc = SlideDocument::create("Sampling.pptx", 960, 540).unwrap();
    let schema = doc.model_schema();
    assert_eq!(schema.field("Shape", "kind").unwrap().sample, Some(false));
}
