#[path = "support/scene_bundle.rs"]
#[allow(dead_code)]
mod fixture;

use xem_studio::scene::{collision, entities};

fn source_collision() -> collision::Collision {
    let components = fixture::components();
    collision::decode_collision(&components[1], components[1].len())
        .expect("decode the invented collision component")
}

fn stable_collision() -> entities::Collision {
    entities::Collision::from_source(&source_collision())
}

fn json<T: serde::Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).expect("serialize stable collision state")
}

#[test]
fn source_conversion_resolves_stable_ids_to_the_exact_original_indices() {
    let source = source_collision();
    let mut authored = stable_collision();
    assert_eq!(authored.layers.len(), 1);
    assert_eq!(authored.layers[0].id, "collision:0");
    assert_eq!(
        authored.layers[0]
            .vertices
            .iter()
            .map(|value| value.id.as_str())
            .collect::<Vec<_>>(),
        [
            "collision:0:vertex:0",
            "collision:0:vertex:1",
            "collision:0:vertex:2"
        ]
    );
    assert_eq!(authored.layers[0].triangles[0].id, "collision:0:triangle:0");
    assert_eq!(
        authored.layers[0].triangles[0].value.vertices,
        [
            "collision:0:vertex:0",
            "collision:0:vertex:1",
            "collision:0:vertex:2"
        ]
    );
    assert_eq!(
        authored.layers[0].triangles[0].value.neighbors,
        [None, None, None]
    );
    assert_eq!(authored.resolve(&source).unwrap(), source);

    assert_eq!(
        authored.vertex_layer("collision:0:vertex:1").unwrap().id,
        "collision:0"
    );
    assert_eq!(
        authored
            .triangle_layer("collision:0:triangle:0")
            .unwrap()
            .id,
        "collision:0"
    );
    authored
        .vertex_layer("collision:0:vertex:1")
        .unwrap()
        .set_vertex("collision:0:vertex:1", [-20, 30, -40])
        .unwrap();
    assert_eq!(
        authored.resolve(&source).unwrap().layers[0].vertices[1],
        [-20, 30, -40, 6]
    );

    assert_eq!(authored.layers[0].id, "collision:0");
    assert!(
        stable_collision().layers[0].vertices[0]
            .id
            .starts_with("collision:0:vertex:")
    );
}

#[test]
fn adding_a_triangle_joins_only_an_oppositely_wound_shared_edge_and_delete_unjoins() {
    let source = source_collision();
    let mut authored = stable_collision();
    let layer = authored.layer("collision:0").unwrap();
    assert_eq!(layer.triangles.len(), 1);

    let layer = authored.layer("collision:0").unwrap();
    let original_face = layer.triangles[0].value.clone();
    let mut authored = stable_collision();
    let layer = authored.layer("collision:0").unwrap();
    assert_eq!(layer.triangles[0].value.neighbors, [None, None, None]);
    authored
        .layer("collision:0")
        .unwrap()
        .add_vertex("scene:1:vertex".into(), [21, -32, 43])
        .unwrap();
    authored
        .layer("collision:0")
        .unwrap()
        .add_triangle(
            "scene:2:triangle".into(),
            [
                "collision:0:vertex:2".into(),
                "collision:0:vertex:1".into(),
                "scene:1:vertex".into(),
            ],
            1,
            0xE9,
        )
        .unwrap();

    let layer = authored.layer("collision:0").unwrap();
    assert_eq!(layer.triangles[0].value.vertices, original_face.vertices);
    assert_eq!(layer.triangles[0].value.attribute, original_face.attribute);
    assert_eq!(
        layer.triangles[0].value.height_bump_raw,
        original_face.height_bump_raw
    );
    assert_eq!(
        layer.triangles[0].value.neighbors[1].as_deref(),
        Some("scene:2:triangle")
    );
    assert_eq!(
        layer.triangles[1].value.neighbors,
        [Some("collision:0:triangle:0".into()), None, None]
    );
    let resolved = authored.resolve(&source).unwrap();
    assert_eq!(resolved.layers[0].triangles[0].neighbors, [-1, 1, -1]);
    assert_eq!(resolved.layers[0].triangles[1].vertices, [2, 1, 3]);
    assert_eq!(resolved.layers[0].triangles[1].neighbors, [0, -1, -1]);
    assert_eq!(resolved.layers[0].vertices[3], [21, -32, 43, 0]);

    authored
        .layer("collision:0")
        .unwrap()
        .delete_triangle("collision:0:triangle:0")
        .unwrap();
    let layer = authored.layer("collision:0").unwrap();
    assert_eq!(layer.triangles.len(), 1);
    assert_eq!(layer.triangles[0].value.neighbors, [None, None, None]);
    assert!(layer.delete_vertex("collision:0:vertex:0").is_ok());
    let resolved = authored.resolve(&source).unwrap();
    assert_eq!(resolved.layers[0].triangles[0].vertices, [1, 0, 2]);
    assert_eq!(resolved.layers[0].triangles[0].neighbors, [-1, -1, -1]);
}

#[test]
fn invalid_edges_vertex_deletion_duplicate_ids_and_third_owners_are_atomic() {
    let mut authored = stable_collision();
    let layer = authored.layer("collision:0").unwrap();
    let before = json(layer);
    assert!(
        authored
            .layer("collision:0")
            .unwrap()
            .delete_vertex("collision:0:vertex:0")
            .is_err()
    );
    assert_eq!(json(authored.layer("collision:0").unwrap()), before);

    let layer = authored.layer("collision:0").unwrap();
    let before = json(layer);
    let layer = authored.layer("collision:0").unwrap();
    assert!(
        layer
            .add_vertex("collision:0:vertex:0".into(), [1, 2, 3])
            .is_err()
    );
    assert_eq!(json(authored.layer("collision:0").unwrap()), before);

    let layer = authored.layer("collision:0").unwrap();
    assert!(
        layer
            .add_triangle(
                "collision:0:triangle:0".into(),
                [
                    "collision:0:vertex:1".into(),
                    "collision:0:vertex:2".into(),
                    "collision:0:vertex:0".into(),
                ],
                1,
                0,
            )
            .is_err()
    );

    let mut bad_edge = stable_collision();
    let layer = bad_edge.layer("collision:0").unwrap();
    let before = json(layer);
    assert!(
        bad_edge
            .layer("collision:0")
            .unwrap()
            .add_triangle(
                "scene:bad-winding".into(),
                [
                    "collision:0:vertex:1".into(),
                    "collision:0:vertex:2".into(),
                    "collision:0:vertex:0".into(),
                ],
                1,
                0,
            )
            .is_err()
    );
    assert_eq!(json(bad_edge.layer("collision:0").unwrap()), before);

    let mut third_owner = stable_collision();
    third_owner
        .layer("collision:0")
        .unwrap()
        .add_vertex("scene:1:vertex".into(), [1, 2, 3])
        .unwrap();
    third_owner
        .layer("collision:0")
        .unwrap()
        .add_triangle(
            "scene:2:triangle".into(),
            [
                "collision:0:vertex:2".into(),
                "collision:0:vertex:1".into(),
                "scene:1:vertex".into(),
            ],
            1,
            0,
        )
        .unwrap();
    let before = json(third_owner.layer("collision:0").unwrap());
    assert!(
        third_owner
            .layer("collision:0")
            .unwrap()
            .add_triangle(
                "scene:3:triangle".into(),
                [
                    "collision:0:vertex:2".into(),
                    "collision:0:vertex:1".into(),
                    "collision:0:vertex:0".into(),
                ],
                1,
                0,
            )
            .is_err()
    );
    assert_eq!(json(third_owner.layer("collision:0").unwrap()), before);
}

#[test]
fn resolve_rejects_dangling_or_duplicate_stable_identities_and_signed_index_overflow() {
    let source = source_collision();
    let mut missing_vertex = stable_collision();
    missing_vertex.layers[0].triangles[0].value.vertices[0] = "missing-vertex".into();
    assert!(missing_vertex.resolve(&source).is_err());

    let mut missing_neighbor = stable_collision();
    missing_neighbor.layers[0].triangles[0].value.neighbors[0] = Some("missing-triangle".into());
    assert!(missing_neighbor.resolve(&source).is_err());

    let mut duplicate_vertex = stable_collision();
    duplicate_vertex.layers[0].vertices[1].id = duplicate_vertex.layers[0].vertices[0].id.clone();
    assert!(duplicate_vertex.resolve(&source).is_err());

    let mut duplicate_triangle = stable_collision();
    let repeated_triangle = duplicate_triangle.layers[0].triangles[0].clone();
    duplicate_triangle.layers[0]
        .triangles
        .push(repeated_triangle);
    assert!(duplicate_triangle.resolve(&source).is_err());

    let mut too_many = stable_collision();
    too_many.layers[0].vertices = (0..32_769)
        .map(|index| xem_studio::scene::domain::Entity {
            id: format!("collision:0:vertex:{index}"),
            value: [0; 4],
        })
        .collect();
    assert!(too_many.resolve(&source).is_err());
}
