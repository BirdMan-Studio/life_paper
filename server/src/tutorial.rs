use crate::{api::auth::authenticate, api::error::ApiError, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TutorialRuntime {
    pub definition: Value,
    pub fired_events: BTreeSet<String>,
    pub completed: bool,
    #[serde(default)]
    pub saw_ejecta: bool,
}

impl TutorialRuntime {
    pub fn new(definition: Value) -> Self {
        let mut state = Self {
            definition,
            fired_events: BTreeSet::new(),
            completed: false,
            saw_ejecta: false,
        };
        state.trigger("world_enter", &BTreeSet::new());
        state
    }

    pub fn trigger(&mut self, trigger: &str, components: &BTreeSet<String>) {
        for event in self.definition["events"].as_array().into_iter().flatten() {
            if event["trigger"] != trigger {
                continue;
            }
            if event["requires"].as_array().is_some_and(|ids| {
                ids.iter()
                    .any(|id| !id.as_str().is_some_and(|id| self.fired_events.contains(id)))
            }) {
                continue;
            }
            if event["condition"]["component"]
                .as_str()
                .is_some_and(|id| !components.contains(id))
            {
                continue;
            }
            if let Some(id) = event["id"].as_str() {
                self.fired_events.insert(id.to_owned());
                if event["complete"] == true {
                    self.completed = true;
                }
            }
        }
    }

    pub fn view(&self) -> Value {
        let panels: Vec<Value> = self.definition["events"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|event| {
                event["id"]
                    .as_str()
                    .is_some_and(|id| self.fired_events.contains(id))
            })
            .filter_map(|event| {
                let mut panel = self.definition["ui"][event["ui"].as_str()?]
                    .as_object()?
                    .clone();
                panel.insert("event_id".into(), event["id"].clone());
                Some(Value::Object(panel))
            })
            .collect();
        json!({"id":self.definition["id"], "completed":self.completed, "fired_events":self.fired_events, "panels":panels, "models":self.definition["models"]})
    }
}

pub fn observe_tick(before: &crate::domain::World, after: &mut crate::domain::World) {
    let Some(state) = after.tutorial.as_mut() else {
        return;
    };
    let changed = after.organisms.iter().any(|(id, organism)| {
        before.organisms.get(id).is_some_and(|old| {
            organism.component_states.iter().any(|(id, component)| {
                old.component_states
                    .get(id)
                    .is_some_and(|old| old.container != component.container)
            })
        })
    });
    if changed {
        state.trigger("container_changed", &BTreeSet::new());
    }
    let used_jet = after
        .matter
        .records
        .iter()
        .skip(before.matter.records.len())
        .any(|record| {
            record.source.is_some()
                && (!record.added_mg.is_empty() || !record.gas_added_ul.is_empty())
        });
    if used_jet {
        state.trigger("jet_used", &BTreeSet::new());
    }
    state.saw_ejecta |= !before.matter.ejecta.is_empty() || !after.matter.ejecta.is_empty();
    if state.saw_ejecta && after.matter.ejecta.is_empty() {
        state.trigger("ejecta_settled", &BTreeSet::new());
    }
}

pub fn build_map(
    definition: &Value,
    terrains: &crate::domain::TerrainRegistry,
) -> Result<crate::domain::World, crate::domain::WorldGenerationError> {
    use crate::domain::{GroundGenerationConfig, MapPosition, TerrainArea, WorldGeneratorEditor};
    let map = &definition["map"];
    let size = map["size_px"].as_u64().unwrap_or(1200) as u32;
    use noise::{NoiseFn, Perlin};
    let seed = map["seed"].as_u64().unwrap_or(0);
    let noise = Perlin::new(seed as u32);
    let spawn: MapPosition = serde_json::from_value(map["spawn"].clone()).unwrap_or(MapPosition {
        x: (size / 2) as i32,
        y: (size / 2) as i32,
    });
    let radius = map["water_radius_px"].as_u64().unwrap_or(80) as u32;
    let lake = MapPosition {
        x: spawn.x + (radius as f64 * 1.75) as i32,
        y: spawn.y - (radius as f64 * 0.45) as i32,
    };
    let mut editor = WorldGeneratorEditor::new(terrains, "wasteland")?;

    // Leave the background procedural; a full grass rectangle hides ground classification.
    for i in 0..12 {
        let angle = i as f64 * std::f64::consts::TAU / 12.0;
        let distance = size as f64 * (0.27 + noise.get([i as f64, 2.7]) * 0.06);
        let point = MapPosition {
            x: (size as f64 / 2.0 + angle.cos() * distance) as i32,
            y: (size as f64 / 2.0 + angle.sin() * distance) as i32,
        };
        let feature_radius =
            (size as f64 * (0.025 + (noise.get([i as f64, 7.2]) + 1.0) * 0.012)) as u32;
        let terrain = match i % 4 {
            0 => "wetland",
            1 => "valley",
            2 => "pond",
            _ => "grassland",
        };
        editor.fill_circle(
            point,
            feature_radius + 12,
            if terrain == "pond" {
                "wetland"
            } else {
                terrain
            },
        )?;
        editor.fill_circle(point, feature_radius, terrain)?;
    }
    editor.fill_circle(spawn, (radius * 2).max(100), "grassland")?;

    let lobes: Vec<_> = (0..7)
        .map(|i| {
            let angle = i as f64 * std::f64::consts::TAU / 7.0;
            let offset = radius as f64 * (0.42 + noise.get([i as f64, 8.1]) * 0.12);
            let point = MapPosition {
                x: lake.x + (angle.cos() * offset) as i32,
                y: lake.y + (angle.sin() * offset * 0.7) as i32,
            };
            let lobe_radius = (radius as f64 * (0.61 + noise.get([i as f64, 4.1]) * 0.2)) as u32;
            (point, lobe_radius)
        })
        .collect();
    for (terrain, margin) in [("wetland", 24), ("shallow_water", 10), ("pond", 0)] {
        for &(point, lobe_radius) in &lobes {
            editor.fill_circle(point, lobe_radius + margin, terrain)?;
        }
    }
    for (x, y, scale) in [(-0.16, 0.05, 0.32), (0.14, -0.07, 0.37), (0.32, 0.09, 0.23)] {
        editor.fill_circle(
            MapPosition {
                x: lake.x + (radius as f64 * x) as i32,
                y: lake.y + (radius as f64 * y) as i32,
            },
            (radius as f64 * scale) as u32,
            "deep_water",
        )?;
    }
    // Dry rocks on the opposite bank provide landmarks without obstructing the spawn.
    for (x, y, r) in [(-168, -88, 22), (-190, -68, 15), (-150, -112, 13)] {
        let point = MapPosition {
            x: spawn.x + x,
            y: spawn.y + y,
        };
        editor
            .fill_circle(point, r + 7, "valley")?
            .fill_circle(point, r, "cliff")?;
    }
    editor
        .fill_circle(spawn, 64, "grassland")?
        .frame_rectangle(MapPosition::default(), size, size, 3, "wall")?;
    Ok(editor.build().with_ground(GroundGenerationConfig {
        seed,
        noise_scale_px: (size / 5).max(96),
        variation_percent: 72,
        area: Some(TerrainArea::Rectangle {
            origin: MapPosition::default(),
            size_x_px: size,
            size_y_px: size,
        }),
        ..GroundGenerationConfig::default()
    }))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/tutorials", get(list))
        .route("/tutorials/{id}/package", get(download))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TutorialPackage {
    pub version: u32,
    pub definition: Value,
    pub world: crate::domain::World,
}

pub fn validate_definition(definition: &Value) -> Result<(), &'static str> {
    let valid_id = |value: &Value| {
        value.as_str().is_some_and(|id| {
            !id.is_empty()
                && id.len() <= 64
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        })
    };
    if !valid_id(&definition["id"])
        || !valid_id(&definition["map"]["level_id"])
        || !definition["title"]
            .as_str()
            .is_some_and(|name| !name.trim().is_empty() && name.chars().count() <= 128)
        || !definition["events"]
            .as_array()
            .is_some_and(|events| events.len() <= 128)
        || !definition["ui"].is_object()
        || !definition["prerequisites"]
            .as_array()
            .is_some_and(|ids| ids.len() <= 64 && ids.iter().all(valid_id))
    {
        return Err("invalid tutorial definition");
    }
    let mut ids = BTreeSet::new();
    for event in definition["events"].as_array().unwrap() {
        if !valid_id(&event["id"])
            || !ids.insert(event["id"].as_str().unwrap())
            || !matches!(
                event["trigger"].as_str(),
                Some(
                    "world_enter"
                        | "organism_created"
                        | "container_changed"
                        | "jet_used"
                        | "ejecta_settled"
                )
            )
            || event.get("ui").is_some_and(|ui| {
                !ui.as_str()
                    .is_some_and(|key| definition["ui"][key].is_object())
            })
        {
            return Err("invalid tutorial event");
        }
    }
    let size = definition["map"]["size_px"]
        .as_u64()
        .filter(|size| (128..=10000).contains(size))
        .ok_or("invalid tutorial map size")?;
    if !definition["map"]["water_radius_px"]
        .as_u64()
        .is_some_and(|radius| radius < size / 2)
        || definition["map"]["seed"].as_u64().is_none()
    {
        return Err("invalid tutorial map");
    }
    let spawn: crate::domain::MapPosition =
        serde_json::from_value(definition["map"]["spawn"].clone())
            .map_err(|_| "invalid tutorial spawn")?;
    if spawn.x <= 0 || spawn.y <= 0 || spawn.x as u64 >= size - 1 || spawn.y as u64 >= size - 1 {
        return Err("invalid tutorial spawn");
    }
    Ok(())
}

async fn download(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    authenticate(&state, &headers).await?;
    Ok(Json(
        json!({"data": package(&id, &state.terrains, &state.components)?}),
    ))
}

pub fn package(
    id: &str,
    terrains: &crate::domain::TerrainRegistry,
    components: &crate::domain::ComponentRegistry,
) -> Result<TutorialPackage, ApiError> {
    let mut definition = find(&id).ok_or(ApiError::NotFound)?;
    definition["models"] = json!([starter_model(components)?]);
    let world = build_map(&definition, terrains).map_err(|_| ApiError::Internal)?;
    Ok(TutorialPackage {
        version: 1,
        definition,
        world,
    })
}

fn starter_model(
    registry: &crate::domain::ComponentRegistry,
) -> Result<crate::transfer::ModelBundle, ApiError> {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let id = uuid::Uuid::from_u128(0x1f9a35303e744134a0cc922c7a54e001);
    let mut model =
        crate::domain::BiologicalOrganism::new(crate::domain::OrganismProgramId::from_uuid(id));
    model.id = id;
    let body = model
        .add_component("small_body_1", registry)
        .map_err(|_| ApiError::Internal)?;
    let jet = model
        .add_component("jet_organ_1", registry)
        .map_err(|_| ApiError::Internal)?;
    model
        .connect(body, jet, registry)
        .map_err(|_| ApiError::Internal)?;
    Ok(crate::transfer::ModelBundle {
        id, name: "教程生物".into(), composition: json!(model),
        files: [("main.py".into(), URL_SAFE_NO_PAD.encode(b"def main(game):\n    for part in game.me.components.values():\n        if part.api.get('jet'):\n            part.jet('water', 20)\n"))].into(), directories: vec![],
    })
}

fn catalog() -> Vec<Value> {
    vec![
        json!({
            "id":"welcome",
            "title":"生命起点",
            "description":"认识世界、身体和第一个生物。",
            "order":10,
            "prerequisites":[],
            "map":{"level_id":"tutorial_welcome","size_px":1200,"water_radius_px":80,"seed":101,"spawn":{"x":600,"y":600}},
            "events":[{"id":"start","trigger":"world_enter","once":true,"ui":"on_start"},{"id":"first-organism","trigger":"organism_created","once":true},{"id":"complete","trigger":"organism_created","once":true,"complete":true,"ui":"on_complete"}],
            "ui":{"on_start":{"panel":"tutorial","title":"欢迎来到生命实验场","steps":["观察地图","创建第一个生物"]},"on_complete":{"panel":"toast","message":"基础教程已完成"}}
        }),
        json!({
            "id":"matter",
            "title":"物质与容器",
            "description":"学习固体、液体和气体容器。",
            "order":20,
            "prerequisites":["welcome"],
            "map":{"level_id":"tutorial_matter","size_px":1600,"water_radius_px":120,"seed":102,"spawn":{"x":800,"y":800}},
            "events":[{"id":"start","trigger":"world_enter","once":true,"ui":"on_start"},{"id":"fill","trigger":"container_changed","once":true,"ui":"on_fill"},{"id":"complete","trigger":"jet_used","requires":["fill"],"once":true,"complete":true,"ui":"on_complete"}],
            "ui":{"on_start":{"panel":"tutorial","title":"物质与容器","steps":["创建教程生物，查看身体容器容量","在初始元素中加入水","观察脚本喷射水"]},"on_fill":{"panel":"tutorial","message":"容器中的物质已变化，观察一次喷射。"},"on_complete":{"panel":"toast","message":"物质教程已完成"}}
        }),
        json!({
            "id":"motion",
            "title":"运动与喷射",
            "description":"理解惯性、阻尼和扩散。",
            "order":30,
            "prerequisites":["matter"],
            "map":{"level_id":"tutorial_motion","size_px":2000,"water_radius_px":160,"seed":103,"spawn":{"x":1000,"y":1000}},
            "events":[{"id":"start","trigger":"world_enter","once":true,"ui":"on_start"},{"id":"eject","trigger":"jet_used","once":true},{"id":"complete","trigger":"ejecta_settled","requires":["eject"],"once":true,"complete":true,"ui":"on_complete"}],
            "ui":{"on_start":{"panel":"tutorial","title":"运动与喷射","steps":["创建教程生物并在初始元素中加入水","观察惯性与阻尼减速","等待喷出物扩散归零"]},"on_complete":{"panel":"toast","message":"运动教程已完成"}}
        }),
    ]
}

async fn list(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Value>, ApiError> {
    authenticate(&state, &headers).await?;
    Ok(Json(json!({"data": catalog()})))
}

pub fn find(id: &str) -> Option<Value> {
    catalog().into_iter().find(|item| item["id"] == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn landscapes_are_seeded_with_safe_spawns_and_layered_water() {
        use crate::domain::{MapPosition, create_default_terrain_registry};
        let terrains = create_default_terrain_registry().unwrap();
        for definition in catalog() {
            let world = build_map(&definition, &terrains).unwrap();
            let serialized = serde_json::to_value(&world).unwrap();
            assert_eq!(
                serialized,
                serde_json::to_value(build_map(&definition, &terrains).unwrap()).unwrap()
            );
            let mut other = definition.clone();
            other["map"]["seed"] = json!(999);
            assert_ne!(
                serialized["terrain_layers"],
                serde_json::to_value(build_map(&other, &terrains).unwrap()).unwrap()["terrain_layers"]
            );
            let spawn: MapPosition =
                serde_json::from_value(definition["map"]["spawn"].clone()).unwrap();
            for dx in -40..=40 {
                for dy in -40..=40 {
                    assert_eq!(
                        world
                            .terrain_at(MapPosition {
                                x: spawn.x + dx,
                                y: spawn.y + dy
                            })
                            .as_str(),
                        "grassland"
                    );
                }
            }
            let mut nearby = BTreeSet::new();
            for dx in (-240..=360).step_by(5) {
                for dy in (-240..=240).step_by(5) {
                    nearby.insert(
                        world
                            .terrain_at(MapPosition {
                                x: spawn.x + dx,
                                y: spawn.y + dy,
                            })
                            .as_str(),
                    );
                }
            }
            for terrain in [
                "grassland",
                "wetland",
                "shallow_water",
                "pond",
                "deep_water",
                "cliff",
            ] {
                assert!(
                    nearby.contains(terrain),
                    "missing {terrain} near {}",
                    definition["id"]
                );
            }
            assert!(world.terrain_layers().len() < 64);
        }
    }

    #[test]
    fn catalog_is_ordered_and_has_runtime_contracts() {
        let entries = catalog();
        assert_eq!(entries.len(), 3);
        assert!(
            entries
                .windows(2)
                .all(|pair| pair[0]["order"].as_i64() < pair[1]["order"].as_i64())
        );
        for entry in entries {
            assert!(entry["map"].is_object());
            assert!(entry["events"].is_array());
            assert!(entry["ui"].is_object());
        }
        assert_eq!(find("matter").unwrap()["prerequisites"][0], "welcome");
        assert!(find("missing").is_none());
    }

    #[test]
    fn event_conditions_dependencies_and_ui_survive_serialization() {
        let mut state = TutorialRuntime::new(find("matter").unwrap());
        assert_eq!(state.view()["panels"][0]["title"], "物质与容器");
        state.trigger("jet_used", &BTreeSet::new());
        assert!(!state.completed);
        state.trigger("container_changed", &BTreeSet::new());
        state.trigger("jet_used", &BTreeSet::new());
        state.trigger("jet_used", &BTreeSet::new());
        assert!(state.completed);
        assert_eq!(state.fired_events.len(), 3);
        let restored: TutorialRuntime =
            serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
        assert_eq!(restored, state);
        assert_eq!(restored.view()["panels"].as_array().unwrap().len(), 3);

        let mut state = TutorialRuntime::new(
            json!({"events":[{"id":"complete","trigger":"organism_created","condition":{"component":"body"},"complete":true}]}),
        );
        state.trigger("organism_created", &BTreeSet::new());
        assert!(!state.completed);
        state.trigger("organism_created", &BTreeSet::from(["body".into()]));
        assert!(state.completed);
    }

    #[test]
    fn motion_requires_actual_ejection_then_disappearance() {
        use crate::domain::{
            create_default_terrain_registry, element::ElementId, world::MatterSource,
        };
        let terrains = create_default_terrain_registry().unwrap();
        let mut before = build_map(&find("motion").unwrap(), &terrains).unwrap();
        before.tutorial = Some(TutorialRuntime::new(find("motion").unwrap()));
        let mut after = before.clone();
        observe_tick(&before, &mut after);
        assert!(!after.tutorial.as_ref().unwrap().completed);
        after
            .matter
            .eject(
                [1000.0, 1000.0],
                [1.0, 0.0],
                [(ElementId::new("water").unwrap(), 1.0)].into(),
                MatterSource {
                    organism_id: uuid::Uuid::new_v4(),
                    component_id: uuid::Uuid::new_v4(),
                },
            )
            .unwrap();
        observe_tick(&before, &mut after);
        assert!(
            after
                .tutorial
                .as_ref()
                .unwrap()
                .fired_events
                .contains("eject")
        );
        assert!(!after.tutorial.as_ref().unwrap().completed);
        for _ in 0..200 {
            before = after.clone();
            after.matter.advance_tick();
            observe_tick(&before, &mut after);
        }
        assert!(after.matter.ejecta.is_empty());
        assert!(after.tutorial.as_ref().unwrap().completed);
    }

    #[test]
    fn tutorial_map_and_progress_persist_and_creation_is_idempotent_per_player() {
        use crate::domain::{MapPosition, WorldDirectory, create_default_terrain_registry};
        let root =
            std::env::temp_dir().join(format!("life-paper-tutorial-{}", uuid::Uuid::new_v4()));
        let terrains = std::sync::Arc::new(create_default_terrain_registry().unwrap());
        let directory = WorldDirectory::new_with_storage(terrains.clone(), &root).unwrap();
        let world = directory
            .create_tutorial_world(7, "welcome", "tutorial_welcome", "Welcome")
            .unwrap();
        assert_eq!(
            directory
                .create_tutorial_world(7, "welcome", "tutorial_welcome", "Welcome")
                .unwrap()
                .id,
            world.id
        );
        assert_ne!(
            directory
                .create_tutorial_world(8, "welcome", "tutorial_welcome", "Welcome")
                .unwrap()
                .id,
            world.id
        );
        let runtime = directory.read_world(world.id, 7).unwrap();
        assert_eq!(runtime.ground_config().unwrap().seed, 101);
        assert_eq!(
            runtime.terrain_at(MapPosition { x: 1200, y: 600 }).as_str(),
            "wasteland"
        );
        assert!(directory.read_world(world.id, 8).is_err());
        drop(directory);
        let directory = WorldDirectory::new_with_storage(terrains, &root).unwrap();
        let resumed = directory.read_world(world.id, 7).unwrap();
        assert_eq!(resumed.tutorial, runtime.tutorial);
        assert_eq!(
            directory
                .create_tutorial_world(7, "welcome", "tutorial_welcome", "Welcome")
                .unwrap()
                .id,
            world.id
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
