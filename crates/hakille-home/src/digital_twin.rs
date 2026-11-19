//! Jumeau Spatial 3D de la Maison (`HomeDigitalTwin`).
//!
//! Relie la géométrie des pièces et appareils (`SpatialPose3D`) aux primitives de
//! `hakille-spatial` (`GazeRay3D`, `DetectedMicroGesture`) et projette le graphe
//! topologique domestique vers `hakille-memory` (`KnowledgeGraph` + `PalaceLayoutEngine`).

use crate::model::{DeviceCategory, HomeCommand, HomeDevice, HomeRiskTier, RoomZone};
use crate::security::HomeRiskClassifier;
use hakille_memory::{KnowledgeGraph, PalaceEdge3D, PalaceLayoutEngine, PalaceNode3D};
use hakille_spatial::{DetectedMicroGesture, GazeRay3D, SwipeDirection};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Résultat d'une intersection entre un rayon de regard 3D et un appareil du jumeau.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpatialDeviceHit {
    pub device_id: String,
    pub device_name: String,
    pub room: String,
    pub distance_m: f32,
    pub ray_offset_m: f32,
}

/// Intention domotique issue d'un regard + micro-geste spatial Zero-Touch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpatialIntent {
    pub target_device_id: String,
    pub command: HomeCommand,
    pub requires_600ms_hold: bool,
    pub rationale: String,
}

/// Jumeau numérique 3D de la maison (pièces + appareils + topologie).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HomeDigitalTwin {
    pub rooms: BTreeMap<String, RoomZone>,
    pub devices: BTreeMap<String, HomeDevice>,
}

impl HomeDigitalTwin {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert_room(&mut self, room: RoomZone) {
        self.rooms.insert(room.id.clone(), room);
    }

    pub fn upsert_device(&mut self, device: HomeDevice) {
        self.devices.insert(device.id.clone(), device);
    }

    pub fn remove_device(&mut self, device_id: &str) -> Option<HomeDevice> {
        self.devices.remove(device_id)
    }

    /// Lance un rayon de regard 3D (`GazeRay3D`) contre les sphères englobantes
    /// (`SpatialPose3D.hit_radius_m`) des appareils dans la limite `max_distance_m`.
    pub fn raycast_gaze(&self, ray: &GazeRay3D, max_distance_m: f32) -> Option<SpatialDeviceHit> {
        let dir = ray.direction_vector();
        let len_sq = dir.x * dir.x + dir.y * dir.y + dir.z * dir.z;
        if len_sq <= 1e-8 {
            return None;
        }
        let inv_len = 1.0 / len_sq.sqrt();
        let dx = dir.x * inv_len;
        let dy = dir.y * inv_len;
        let dz = dir.z * inv_len;

        let mut best: Option<SpatialDeviceHit> = None;

        for device in self.devices.values() {
            let ocx = device.pose_3d.x_m - ray.origin.x;
            let ocy = device.pose_3d.y_m - ray.origin.y;
            let ocz = device.pose_3d.z_m - ray.origin.z;

            // Projection scalaire t le long du rayon unitaire
            let t = ocx * dx + ocy * dy + ocz * dz;
            if t < 0.0 || t > max_distance_m {
                continue;
            }

            let closest_x = ray.origin.x + t * dx;
            let closest_y = ray.origin.y + t * dy;
            let closest_z = ray.origin.z + t * dz;

            let rx = device.pose_3d.x_m - closest_x;
            let ry = device.pose_3d.y_m - closest_y;
            let rz = device.pose_3d.z_m - closest_z;
            let perp_dist = (rx * rx + ry * ry + rz * rz).sqrt();

            if perp_dist <= device.pose_3d.hit_radius_m {
                let candidate = SpatialDeviceHit {
                    device_id: device.id.clone(),
                    device_name: device.name.clone(),
                    room: device.room.clone(),
                    distance_m: t,
                    ray_offset_m: perp_dist,
                };
                match &best {
                    Some(prev) if prev.distance_m <= candidate.distance_m => {}
                    _ => best = Some(candidate),
                }
            }
        }

        best
    }

    /// Traduit un micro-geste spatial (`Pinch`, `Swipe`, `Silence`) combiné à l'appareil
    /// ciblé par le regard en une commande domotique explicite.
    ///
    /// Les actions Tier 3 (serrure, garage, disjoncteur…) exigent explicitement
    /// `requires_600ms_hold = true` et un maintien confirmé ≥ 600 ms.
    pub fn interpret_micro_gesture(
        &self,
        gesture: &DetectedMicroGesture,
        gaze_target: Option<&SpatialDeviceHit>,
    ) -> Option<SpatialIntent> {
        match gesture {
            DetectedMicroGesture::IndexThumbPinch {
                confirmed,
                duration_ms,
                ..
            } => {
                let hit = gaze_target?;
                let dev = self.devices.get(&hit.device_id)?;
                if !confirmed {
                    return None;
                }
                let cmd = match dev.category {
                    DeviceCategory::DoorLock => {
                        if dev.state.locked.unwrap_or(true) {
                            HomeCommand::UnlockDoor
                        } else {
                            HomeCommand::LockDoor
                        }
                    }
                    DeviceCategory::GarageDoor => {
                        if dev.state.position_pct.unwrap_or(0) > 10 {
                            HomeCommand::CloseGarage
                        } else {
                            HomeCommand::OpenGarage
                        }
                    }
                    _ => HomeCommand::Toggle,
                };
                let tier = HomeRiskClassifier::classify(dev.category, &cmd);
                let requires_hold = tier == HomeRiskTier::Tier3CriticalExecution;
                if requires_hold && *duration_ms < 600 {
                    return Some(SpatialIntent {
                        target_device_id: dev.id.clone(),
                        command: cmd,
                        requires_600ms_hold: true,
                        rationale: format!(
                            "Regard sur '{}' + pincement {} ms (< 600 ms : armement carte d'action requis)",
                            dev.name, duration_ms
                        ),
                    });
                }
                Some(SpatialIntent {
                    target_device_id: dev.id.clone(),
                    command: cmd,
                    requires_600ms_hold: requires_hold,
                    rationale: format!(
                        "Regard sur '{}' ({:.2} m) + pincement confirmé ({} ms)",
                        dev.name, hit.distance_m, duration_ms
                    ),
                })
            }
            DetectedMicroGesture::LateralSwipe { direction, .. } => {
                let hit = gaze_target?;
                let dev = self.devices.get(&hit.device_id)?;
                let current_level =
                    dev.state.level_pct.or(dev.state.position_pct).unwrap_or(50) as i16;
                let cmd = match (dev.category, direction) {
                    (DeviceCategory::CoverBlind, SwipeDirection::Up)
                    | (DeviceCategory::CoverBlind, SwipeDirection::Right) => {
                        HomeCommand::SetCoverPosition {
                            position_pct: (current_level + 20).clamp(0, 100) as u8,
                        }
                    }
                    (DeviceCategory::CoverBlind, SwipeDirection::Down)
                    | (DeviceCategory::CoverBlind, SwipeDirection::Left) => {
                        HomeCommand::SetCoverPosition {
                            position_pct: (current_level - 20).clamp(0, 100) as u8,
                        }
                    }
                    (DeviceCategory::Thermostat | DeviceCategory::Hvac, SwipeDirection::Up) => {
                        let base = dev.state.target_temperature_c.unwrap_or(20.0);
                        HomeCommand::SetTemperature {
                            celsius: (base + 1.0).clamp(10.0, 28.0),
                        }
                    }
                    (DeviceCategory::Thermostat | DeviceCategory::Hvac, SwipeDirection::Down) => {
                        let base = dev.state.target_temperature_c.unwrap_or(20.0);
                        HomeCommand::SetTemperature {
                            celsius: (base - 1.0).clamp(10.0, 28.0),
                        }
                    }
                    (_, SwipeDirection::Up | SwipeDirection::Right) => HomeCommand::SetLevel {
                        level_pct: (current_level + 20).clamp(0, 100) as u8,
                    },
                    (_, SwipeDirection::Down | SwipeDirection::Left) => HomeCommand::SetLevel {
                        level_pct: (current_level - 20).clamp(0, 100) as u8,
                    },
                };
                let tier = HomeRiskClassifier::classify(dev.category, &cmd);
                Some(SpatialIntent {
                    target_device_id: dev.id.clone(),
                    command: cmd,
                    requires_600ms_hold: tier == HomeRiskTier::Tier3CriticalExecution,
                    rationale: format!("Regard sur '{}' + balayage {:?}", dev.name, direction),
                })
            }
            DetectedMicroGesture::SilenceGesture { active: true, .. } => {
                // Couper le premier haut-parleur ou lecteur média actif
                let media_dev = self.devices.values().find(|d| {
                    matches!(
                        d.category,
                        DeviceCategory::Speaker | DeviceCategory::MediaPlayer
                    ) && d.state.on.unwrap_or(false)
                })?;
                Some(SpatialIntent {
                    target_device_id: media_dev.id.clone(),
                    command: HomeCommand::SetPower { on: false },
                    requires_600ms_hold: false,
                    rationale: format!(
                        "Geste de silence détecté : mise en sourdine de '{}'",
                        media_dev.name
                    ),
                })
            }
            DetectedMicroGesture::SilenceGesture { active: false, .. } => None,
        }
    }

    /// Projette la topologie de la maison (pièces, appareils, protocoles, états) dans le
    /// graphe de connaissances cognitif (`hakille-memory::KnowledgeGraph`).
    pub fn project_to_knowledge_graph(&self, graph: &mut KnowledgeGraph) -> usize {
        let mut count = 0;
        let now = hakille_core::Timestamp::now();
        for room in self.rooms.values() {
            let s = graph.resolve_subject(&format!("room:{}", room.id));
            let p_name = graph.resolve_predicate("room_name");
            let t = hakille_memory::Triplet {
                id: KnowledgeGraph::deterministic_triplet_id(
                    &s,
                    &p_name,
                    &hakille_memory::NodeValue::Text(room.name.clone()),
                ),
                subject: s,
                predicate: p_name,
                object: hakille_memory::NodeValue::Text(room.name.clone()),
                metadata: hakille_memory::TripletMetadata {
                    weight: 0.9,
                    created_at: now.clone(),
                    last_activated: now.clone(),
                    source: hakille_memory::TripletSource::System,
                    confidence: 0.95,
                    context: hakille_memory::ContextPartition::Default,
                    trust_level: hakille_core::TrustLevel::System,
                    decay_rate: 0.005,
                },
                node_type: Some(hakille_memory::NodeType::Fact),
            };
            graph.insert(t);
            count += 1;
        }

        for dev in self.devices.values() {
            let subj = graph.resolve_subject(&format!("device:{}", dev.id));
            let facts = [
                ("located_in", format!("room:{}", dev.room)),
                ("uses_protocol", dev.protocol.as_str().to_string()),
                ("has_category", dev.category.as_str().to_string()),
                ("has_status", dev.status.label()),
            ];
            for (pred, val) in facts {
                let p = graph.resolve_predicate(pred);
                let obj = hakille_memory::NodeValue::Text(val);
                let tid = KnowledgeGraph::deterministic_triplet_id(&subj, &p, &obj);
                graph.insert(hakille_memory::Triplet {
                    id: tid,
                    subject: subj.clone(),
                    predicate: p,
                    object: obj,
                    metadata: hakille_memory::TripletMetadata {
                        weight: 0.92,
                        created_at: now.clone(),
                        last_activated: now.clone(),
                        source: hakille_memory::TripletSource::System,
                        confidence: 0.95,
                        context: hakille_memory::ContextPartition::Default,
                        trust_level: hakille_core::TrustLevel::System,
                        decay_rate: 0.01,
                    },
                    node_type: Some(hakille_memory::NodeType::Fact),
                });
                count += 1;
            }
        }
        count
    }

    /// Génère une scène 3D compatible avec le moteur de rendu `PalaceSceneRenderer`.
    pub fn to_palace_3d(&self) -> PalaceLayoutEngine {
        let mut engine = PalaceLayoutEngine::new();
        for room in self.rooms.values() {
            engine.add_node(PalaceNode3D::new(
                format!("room:{}", room.id),
                room.name.clone(),
                [
                    room.center_m[0] * 18.0,
                    room.center_m[1] * 18.0,
                    room.center_m[2] * 18.0,
                ],
                "home_room",
                0.9,
            ));
        }

        for dev in self.devices.values() {
            let mut node = PalaceNode3D::new(
                format!("device:{}", dev.id),
                format!("{} [{}]", dev.name, dev.protocol.as_str()),
                [
                    dev.pose_3d.x_m * 18.0,
                    dev.pose_3d.y_m * 18.0,
                    dev.pose_3d.z_m * 18.0,
                ],
                dev.room.clone(),
                if dev.status.is_online() { 0.95 } else { 0.45 },
            );
            if matches!(
                dev.status,
                crate::model::ConnectionStatus::Error(_)
                    | crate::model::ConnectionStatus::Offline(_)
            ) {
                node = node.with_contradiction(0.8);
            }
            engine.add_node(node);
            if self.rooms.contains_key(&dev.room) {
                engine.add_edge(PalaceEdge3D::new(
                    format!("device:{}", dev.id),
                    format!("room:{}", dev.room),
                    "located_in",
                    0.9,
                    false,
                ));
            }
        }
        engine
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ConnectionStatus, ProtocolKind, SpatialPose3D};
    use hakille_spatial::{Handedness, Point3D, Vector3D};

    #[test]
    fn gaze_raycast_and_pinch_targets_exact_3d_device() {
        let mut twin = HomeDigitalTwin::new();
        twin.upsert_room(RoomZone::new("salon", "Salon", 0, [0.0, 1.4, -3.0]));

        let mut lamp = HomeDevice::new(
            "matter-lamp-salon",
            "Lampe Salon",
            "salon",
            ProtocolKind::Matter,
            DeviceCategory::ColorLight,
            "192.168.1.40:5540",
        );
        lamp.pose_3d = SpatialPose3D::new(0.0, 1.5, -3.0).with_radius(0.5);
        lamp.status = ConnectionStatus::PairedOnline;
        twin.upsert_device(lamp);

        let ray = GazeRay3D::from_vector(
            Point3D::new(0.0, 1.5, 0.0),
            Vector3D::new(0.0, 0.0, -1.0),
            false,
        );
        let hit = twin.raycast_gaze(&ray, 8.0).expect("Doit toucher la lampe");
        assert_eq!(hit.device_id, "matter-lamp-salon");
        assert!((hit.distance_m - 3.0).abs() < 1e-4);

        let pinch = DetectedMicroGesture::IndexThumbPinch {
            hand: Handedness::Right,
            pinch_distance_norm: 0.12,
            duration_ms: 200,
            confirmed: true,
        };
        let intent = twin.interpret_micro_gesture(&pinch, Some(&hit)).unwrap();
        assert_eq!(intent.command, HomeCommand::Toggle);
        assert!(!intent.requires_600ms_hold);
    }

    #[test]
    fn projects_digital_twin_into_knowledge_graph_and_palace() {
        let mut twin = HomeDigitalTwin::new();
        twin.upsert_room(RoomZone::new("cuisine", "Cuisine", 0, [4.0, 1.4, -2.0]));
        twin.upsert_device(HomeDevice::new(
            "zigbee-plug-1",
            "Prise Machine Café",
            "cuisine",
            ProtocolKind::Zigbee,
            DeviceCategory::Switch,
            "/dev/ttyUSB0",
        ));

        let mut graph = KnowledgeGraph::new();
        let inserted = twin.project_to_knowledge_graph(&mut graph);
        assert!(inserted >= 5);

        let palace = twin.to_palace_3d();
        assert_eq!(palace.nodes.len(), 2);
        assert_eq!(palace.edges.len(), 1);
    }
}
