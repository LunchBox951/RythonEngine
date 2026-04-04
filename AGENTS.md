# Source Map

```
RythonEngine/
├── .claude/
│   ├── remake_agent.py
│   └── settings.json
├── crates/
│   ├── rython-audio/
│   │   ├── src/
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   ├── rython-cli/
│   │   ├── src/
│   │   │   └── main.rs
│   │   └── Cargo.toml
│   ├── rython-core/
│   │   ├── src/
│   │   │   ├── components.rs
│   │   │   ├── config.rs
│   │   │   ├── errors.rs
│   │   │   ├── events.rs
│   │   │   ├── lib.rs
│   │   │   ├── math.rs
│   │   │   ├── scheduler_trait.rs
│   │   │   └── types.rs
│   │   ├── tests/
│   │   │   └── acceptance.rs
│   │   └── Cargo.toml
│   ├── rython-ecs/
│   │   ├── src/
│   │   │   ├── systems/
│   │   │   │   ├── light.rs
│   │   │   │   ├── mod.rs
│   │   │   │   ├── render.rs
│   │   │   │   └── transform.rs
│   │   │   ├── command.rs
│   │   │   ├── component.rs
│   │   │   ├── entity.rs
│   │   │   ├── event_bus.rs
│   │   │   ├── hierarchy.rs
│   │   │   ├── lib.rs
│   │   │   ├── scene.rs
│   │   │   └── tests.rs
│   │   └── Cargo.toml
│   ├── rython-editor/
│   │   ├── src/
│   │   │   ├── panels/
│   │   │   │   ├── asset_browser.rs
│   │   │   │   ├── component_inspector.rs
│   │   │   │   ├── console.rs
│   │   │   │   ├── mod.rs
│   │   │   │   ├── scene_hierarchy.rs
│   │   │   │   ├── script_panel.rs
│   │   │   │   ├── ui_editor.rs
│   │   │   │   ├── viewport_panel.rs
│   │   │   │   └── welcome.rs
│   │   │   ├── project/
│   │   │   │   ├── format.rs
│   │   │   │   ├── io.rs
│   │   │   │   ├── mod.rs
│   │   │   │   └── scaffold.rs
│   │   │   ├── state/
│   │   │   │   ├── clipboard.rs
│   │   │   │   ├── mod.rs
│   │   │   │   ├── preferences.rs
│   │   │   │   ├── project.rs
│   │   │   │   ├── selection.rs
│   │   │   │   ├── undo.rs
│   │   │   │   └── viewport.rs
│   │   │   ├── viewport/
│   │   │   │   ├── camera_controller.rs
│   │   │   │   ├── gizmo.rs
│   │   │   │   ├── mod.rs
│   │   │   │   ├── offscreen.rs
│   │   │   │   └── picking.rs
│   │   │   ├── app.rs
│   │   │   ├── lib.rs
│   │   │   └── main.rs
│   │   └── Cargo.toml
│   ├── rython-engine/
│   │   ├── src/
│   │   │   ├── builder.rs
│   │   │   └── lib.rs
│   │   ├── tests/
│   │   │   └── spec_tests.rs
│   │   └── Cargo.toml
│   ├── rython-input/
│   │   ├── src/
│   │   │   ├── bitset.rs
│   │   │   ├── controller.rs
│   │   │   ├── events.rs
│   │   │   ├── input_map.rs
│   │   │   ├── lib.rs
│   │   │   └── snapshot.rs
│   │   ├── tests/
│   │   │   └── acceptance.rs
│   │   └── Cargo.toml
│   ├── rython-modules/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── loader.rs
│   │   │   ├── module.rs
│   │   │   ├── registry.rs
│   │   │   └── state.rs
│   │   ├── tests/
│   │   │   └── acceptance.rs
│   │   └── Cargo.toml
│   ├── rython-physics/
│   │   ├── src/
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   ├── rython-renderer/
│   │   ├── src/
│   │   │   ├── camera.rs
│   │   │   ├── command.rs
│   │   │   ├── config.rs
│   │   │   ├── gpu.rs
│   │   │   ├── lib.rs
│   │   │   ├── light.rs
│   │   │   ├── queue.rs
│   │   │   ├── shaders.rs
│   │   │   └── shadow.rs
│   │   ├── tests/
│   │   │   └── acceptance.rs
│   │   └── Cargo.toml
│   ├── rython-resources/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   └── tangents.rs
│   │   └── Cargo.toml
│   ├── rython-scheduler/
│   │   ├── src/
│   │   │   ├── frame_pacer.rs
│   │   │   ├── group.rs
│   │   │   ├── lib.rs
│   │   │   ├── scheduler.rs
│   │   │   └── task.rs
│   │   ├── tests/
│   │   │   └── acceptance.rs
│   │   └── Cargo.toml
│   ├── rython-scripting/
│   │   ├── src/
│   │   │   ├── bridge/
│   │   │   │   ├── audio.rs
│   │   │   │   ├── camera.rs
│   │   │   │   ├── engine.rs
│   │   │   │   ├── entity.rs
│   │   │   │   ├── input.rs
│   │   │   │   ├── job_handle.rs
│   │   │   │   ├── mod.rs
│   │   │   │   ├── physics.rs
│   │   │   │   ├── renderer.rs
│   │   │   │   ├── resources.rs
│   │   │   │   ├── scene.rs
│   │   │   │   ├── scheduler.rs
│   │   │   │   ├── task.rs
│   │   │   │   ├── time.rs
│   │   │   │   ├── types.rs
│   │   │   │   └── ui.rs
│   │   │   ├── component.rs
│   │   │   ├── config.rs
│   │   │   ├── lib.rs
│   │   │   └── system.rs
│   │   ├── tests/
│   │   │   └── acceptance.rs
│   │   └── Cargo.toml
│   ├── rython-ui/
│   │   ├── src/
│   │   │   ├── animator.rs
│   │   │   ├── commands.rs
│   │   │   ├── lib.rs
│   │   │   ├── manager.rs
│   │   │   ├── theme.rs
│   │   │   └── widget.rs
│   │   └── Cargo.toml
│   └── rython-window/
│       ├── src/
│       │   ├── lib.rs
│       │   ├── raw_events.rs
│       │   ├── tests.rs
│       │   └── window_module.rs
│       └── Cargo.toml
├── docs/
│   ├── engine/
│   │   └── README.md
│   └── game/
│       └── README.md
├── game/
│   ├── assets/
│   │   ├── fonts/
│   │   │   └── fallback.ttf
│   │   ├── models/
│   │   │   ├── cube.obj
│   │   │   └── DOWNLOAD_README.md
│   │   ├── music/
│   │   │   ├── arena1.mp3
│   │   │   ├── arena2.mp3
│   │   │   ├── jingle_levelup.ogg
│   │   │   ├── jingle_menu_01.ogg
│   │   │   ├── jingle_menu_02.ogg
│   │   │   ├── jingle_transition.ogg
│   │   │   ├── jingle_win.ogg
│   │   │   └── menu.mp3
│   │   ├── sounds/
│   │   │   ├── sfx/
│   │   │   │   ├── chop.ogg
│   │   │   │   ├── coin_pickup_01.ogg
│   │   │   │   ├── coin_pickup_02.ogg
│   │   │   │   ├── door_close.ogg
│   │   │   │   ├── door_open.ogg
│   │   │   │   ├── footstep_01.ogg
│   │   │   │   ├── footstep_02.ogg
│   │   │   │   ├── footstep_03.ogg
│   │   │   │   ├── impact_light_01.ogg
│   │   │   │   ├── impact_light_02.ogg
│   │   │   │   ├── impact_plank_01.ogg
│   │   │   │   ├── impact_wood_01.ogg
│   │   │   │   ├── land_soft_01.ogg
│   │   │   │   ├── land_soft_02.ogg
│   │   │   │   └── metal_click.ogg
│   │   │   └── ui/
│   │   │       ├── click_01.ogg
│   │   │       ├── click_02.ogg
│   │   │       ├── close_01.ogg
│   │   │       ├── confirm_01.ogg
│   │   │       ├── confirm_02.ogg
│   │   │       ├── error_01.ogg
│   │   │       ├── select_01.ogg
│   │   │       ├── select_02.ogg
│   │   │       ├── switch_01.ogg
│   │   │       └── toggle_01.ogg
│   │   └── textures/
│   │       ├── Dark/
│   │       │   ├── dark_box.png
│   │       │   ├── dark_box_alt1.png
│   │       │   ├── dark_box_alt2.png
│   │       │   ├── dark_box_grid.png
│   │       │   ├── dark_door_sign.png
│   │       │   ├── dark_floor_grid.png
│   │       │   ├── dark_stairs_sign.png
│   │       │   ├── dark_wall.png
│   │       │   ├── dark_wall_alt1.png
│   │       │   ├── dark_wall_alt2.png
│   │       │   ├── dark_wall_grid.png
│   │       │   ├── dark_wall_sign.png
│   │       │   └── dark_window_sign.png
│   │       ├── Green/
│   │       │   ├── green_box.png
│   │       │   ├── green_box_alt1.png
│   │       │   ├── green_box_alt2.png
│   │       │   ├── green_box_grid.png
│   │       │   ├── green_door_sign.png
│   │       │   ├── green_floor_grid.png
│   │       │   ├── green_stairs_sign.png
│   │       │   ├── green_wall.png
│   │       │   ├── green_wall_alt1.png
│   │       │   ├── green_wall_alt2.png
│   │       │   ├── green_wall_grid.png
│   │       │   ├── green_wall_sign.png
│   │       │   └── green_window_sign.png
│   │       ├── Light/
│   │       │   ├── light_box.png
│   │       │   ├── light_box_alt1.png
│   │       │   ├── light_box_alt2.png
│   │       │   ├── light_box_grid.png
│   │       │   ├── light_door_sign.png
│   │       │   ├── light_floor_grid.png
│   │       │   ├── light_stairs_sign.png
│   │       │   ├── light_wall.png
│   │       │   ├── light_wall_alt1.png
│   │       │   ├── light_wall_alt2.png
│   │       │   ├── light_wall_grid.png
│   │       │   ├── light_wall_sign.png
│   │       │   └── light_window_sign.png
│   │       ├── Orange/
│   │       │   ├── orange_box.png
│   │       │   ├── orange_box_alt1.png
│   │       │   ├── orange_box_alt2.png
│   │       │   ├── orange_box_grid.png
│   │       │   ├── orange_door_sign.png
│   │       │   ├── orange_floor_grid.png
│   │       │   ├── orange_stairs_sign.png
│   │       │   ├── orange_wall.png
│   │       │   ├── orange_wall_alt1.png
│   │       │   ├── orange_wall_alt2.png
│   │       │   ├── orange_wall_grid.png
│   │       │   ├── orange_wall_sign.png
│   │       │   └── orange_window_sign.png
│   │       ├── Purple/
│   │       │   ├── purple_box.png
│   │       │   ├── purple_box_alt1.png
│   │       │   ├── purple_box_alt2.png
│   │       │   ├── purple_box_grid.png
│   │       │   ├── purple_door_sign.png
│   │       │   ├── purple_floor_grid.png
│   │       │   ├── purple_stairs_sign.png
│   │       │   ├── purple_wall.png
│   │       │   ├── purple_wall_alt1.png
│   │       │   ├── purple_wall_alt2.png
│   │       │   ├── purple_wall_grid.png
│   │       │   ├── purple_wall_sign.png
│   │       │   └── purple_window_sign.png
│   │       └── Red/
│   │           ├── red_box.png
│   │           ├── red_box_alt1.png
│   │           ├── red_box_alt2.png
│   │           ├── red_box_grid.png
│   │           ├── red_door_sign.png
│   │           ├── red_floor_grid.png
│   │           ├── red_stairs_sign.png
│   │           ├── red_wall.png
│   │           ├── red_wall_alt1.png
│   │           ├── red_wall_alt2.png
│   │           ├── red_wall_grid.png
│   │           ├── red_wall_sign.png
│   │           └── red_window_sign.png
│   ├── scenes/
│   │   ├── arena_1.json
│   │   ├── arena_2.json
│   │   └── arena_3.json
│   ├── scripts/
│   │   ├── levels/
│   │   │   ├── __init__.py
│   │   │   ├── arena_1.py
│   │   │   ├── arena_2.py
│   │   │   └── arena_3.py
│   │   ├── menus/
│   │   │   ├── __init__.py
│   │   │   ├── hud.py
│   │   │   ├── main_menu.py
│   │   │   ├── pause_menu.py
│   │   │   └── settings_menu.py
│   │   ├── npc/
│   │   │   ├── __init__.py
│   │   │   └── skeleton.py
│   │   ├── __init__.py
│   │   ├── camera_follow.py
│   │   ├── enemies.py
│   │   ├── game_state.py
│   │   ├── level_builder.py
│   │   ├── main.py
│   │   └── player.py
│   ├── ui/
│   │   ├── main_menu.json
│   │   ├── pause_menu.json
│   │   └── settings_menu.json
│   ├── __init__.py
│   ├── CREDITS.md
│   └── project.json
├── rython/
│   ├── __init__.py
│   ├── _audio.py
│   ├── _camera.py
│   ├── _decorators.py
│   ├── _engine.py
│   ├── _entity.py
│   ├── _input.py
│   ├── _physics.py
│   ├── _renderer.py
│   ├── _resources.py
│   ├── _scene.py
│   ├── _scheduler.py
│   ├── _stubs.py
│   ├── _time.py
│   ├── _types.py
│   ├── _ui.py
│   └── py.typed
├── .gitignore
├── AGENTS.md
├── Cargo.lock
├── Cargo.toml
├── Makefile
├── pyproject.toml
└── README.md
```
