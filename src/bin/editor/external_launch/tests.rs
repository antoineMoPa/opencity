use super::*;

#[test]
fn unsaved_default_snapshot_materializes_embedded_elevation_and_cleans_it_up() {
    let program = boot_program();
    let original = program.clone();
    let snapshot = LaunchSnapshot::create(&program, None).unwrap();
    let path = snapshot.0.clone();
    let asset = snapshot.1.clone().expect("embedded terrain exported");
    let contents = parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let spec = opencity::heightmap::ElevationMapSpec::from_program(&contents)
        .unwrap()
        .unwrap();
    assert_eq!(path.parent().unwrap().join(&spec.path), asset);
    assert_eq!(
        std::fs::read(&asset).unwrap(),
        include_bytes!("../../../../examples/default_elevation.png")
    );
    let elevation = spec
        .load(path.parent())
        .expect("external application can load snapshot terrain");
    let expected = elevation_map::load(&program, None).unwrap().unwrap();
    #[cfg(feature = "game-runtime")]
    opencity::runtime::City::load(&path).expect("game runtime loads the complete unsaved default city");
    assert_eq!(
        elevation.height_at([-13.566294, 7.1542263]),
        expected.height_at([-13.566294, 7.1542263])
    );
    assert_eq!(program, original);
    drop(snapshot);
    assert!(!path.exists());
    assert!(!asset.exists());
}

#[test]
fn launch_spinner_replaces_loading_window_and_errors_are_centered() {
    let mut editor = crate::tests::sample_editor();
    let ctx = egui::Context::default();
    let (sender, receiver) = std::sync::mpsc::channel();
    editor.minimal.launch_feedback = Some(LaunchFeedback {
        message: "Starting application".into(),
        completion: Some(receiver),
        waiting: true,
    });
    let draw = |editor: &mut RoadEditor| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900., 650.),
                )),
                ..Default::default()
            },
            |ui| editor.launch_feedback_ui(ui.ctx()),
        );
        output.textures_delta.clear();
        output
    };
    let output = draw(&mut editor);
    assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Text(text) if text.galley.job.text.contains("Starting application")
            || text.galley.job.text == "External application")));
    sender.send(LaunchEvent::Ready).unwrap();
    let _ = draw(&mut editor);
    assert!(!editor.minimal.launch_feedback.as_ref().unwrap().waiting);
    sender
        .send(LaunchEvent::Finished(Err("Failed to load map".into())))
        .unwrap();
    let _ = draw(&mut editor);
    let _ = draw(&mut editor);
    let rect = ctx
        .memory(|memory| memory.area_rect(egui::Id::new("external-launch-error")))
        .unwrap();
    assert!(
        (rect.center() - egui::pos2(450., 325.)).length() < 2.,
        "{rect:?}"
    );
    assert_eq!(editor.status, "Failed to load map");
}

#[test]
fn unsaved_snapshot_preserves_asset_directory_and_is_removed_on_drop() {
    let program = parse("note=annotation_text(\"current edits\",0,0);").unwrap();
    let map = std::env::temp_dir().join("original map.road");
    let snapshot = LaunchSnapshot::create(&program, Some(&map)).unwrap();
    let path = snapshot.0.clone();
    assert_eq!(path.parent(), map.parent());
    assert_eq!(
        parse(&std::fs::read_to_string(&path).unwrap()).unwrap(),
        program
    );
    drop(snapshot);
    assert!(!path.exists());
}

#[test]
fn explicit_configuration_omitted_missing_and_invalid() {
    assert!(LaunchAction::load(None).unwrap().is_none());
    let missing = PathBuf::from(format!(
        "/tmp/opencity-missing-launch-action-{}.json",
        std::process::id()
    ));
    assert!(LaunchAction::load(Some(&missing))
        .unwrap_err()
        .contains("Cannot read launch action"));
    for source in [
        "not json",
        "[]",
        "{}",
        r#"{"label":"Open","program":"tool","args":[1]}"#,
        r#"{"label":"Open","program":"tool","args":["{unknown}"]}"#,
        r#"{"label":"Open","program":"tool","args":["{map"]}"#,
        r#"{"label":"Open","program":"tool","args":[],"typo":true}"#,
        r#"{"label":"Open","program":"tool","args":[],"ready_message":""}"#,
        r#"{"label":"Open","program":"tool","args":[],"ready_message":true}"#,
    ] {
        assert!(
            LaunchAction::parse(source, PathBuf::from("/tmp")).is_err(),
            "{source}"
        );
    }
}

#[test]
fn command_expands_explicit_tokens_without_shell_or_recursive_substitution() {
    let config_dir = std::env::temp_dir().join("config directory {x}");
    let action = LaunchAction::parse(
        r#"{"label":"Inspect here","program":"{config_dir}/external viewer","args":["--map","{map}","{x},{y}","$(touch danger); quoted ' words", "{map}:{map}"]}"#,
        config_dir.clone(),
    ).unwrap();
    let map = std::env::temp_dir()
        .join("map directory {y}")
        .join("city & map.road");
    let map_text = map.to_str().unwrap();
    let program = format!("{}/external viewer", config_dir.display());
    let repeated_map = format!("{map_text}:{map_text}");
    let command = action.command(&map, [1.25, -4.5]).unwrap();
    assert_eq!(command.get_program(), program.as_str());
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        vec![
            "--map",
            map_text,
            "1.25,-4.5",
            "$(touch danger); quoted ' words",
            repeated_map.as_str()
        ]
    );
}

#[test]
fn relative_config_path_resolves_directory_and_map_to_absolute_paths() {
    let filename = format!("opencity-launch-test-{}.json", std::process::id());
    let directory = std::env::current_dir().unwrap().join("target");
    std::fs::create_dir_all(&directory).unwrap();
    let config = directory.join(&filename);
    std::fs::write(
        &config,
        r#"{"label":"Inspect","program":"viewer","args":["{config_dir}","{map}"]}"#,
    )
    .unwrap();
    // Resolve an explicit path relative to the current directory, without changing global state.
    let relative = PathBuf::from("target").join(&filename);
    let action = LaunchAction::load(Some(&relative)).unwrap().unwrap();
    let command = action
        .command(std::path::Path::new("a map.road"), [0., 0.])
        .unwrap();
    let args = command.get_args().collect::<Vec<_>>();
    assert_eq!(
        std::fs::canonicalize(args[0]).unwrap(),
        std::fs::canonicalize(directory).unwrap()
    );
    assert_eq!(args[1], std::env::current_dir().unwrap().join("a map.road"));
    std::fs::remove_file(config).unwrap();
}
