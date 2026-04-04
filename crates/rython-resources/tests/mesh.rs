//! Integration tests for mesh-related utilities: compute_tangents behavior,
//! edge cases, and orthogonality of the TBN frame.

use rython_resources::tangents::compute_tangents;
use rython_resources::{generate_cube, Vertex};

// ── Helper ────────────────────────────────────────────────────────────────────

fn zero_vertex() -> Vertex {
    Vertex {
        position: [0.0, 0.0, 0.0],
        normal: [0.0, 1.0, 0.0],
        uv: [0.0, 0.0],
        tangent: [0.0, 0.0, 0.0],
        bitangent: [0.0, 0.0, 0.0],
        _pad: [0.0, 0.0],
    }
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length3(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

// ── Empty / degenerate inputs ─────────────────────────────────────────────────

#[test]
fn compute_tangents_empty_vertices_is_noop() {
    let mut verts: Vec<Vertex> = vec![];
    let indices: Vec<u32> = vec![];
    compute_tangents(&mut verts, &indices); // must not panic
}

#[test]
fn compute_tangents_fewer_than_three_indices_is_noop() {
    let mut verts = vec![zero_vertex(), zero_vertex()];
    let indices: Vec<u32> = vec![0, 1];
    compute_tangents(&mut verts, &indices); // must not panic
}

#[test]
fn compute_tangents_out_of_range_index_is_skipped() {
    let mut verts = vec![zero_vertex(); 3];
    // Index 99 is out of range — should be silently skipped.
    let indices: Vec<u32> = vec![0, 1, 99];
    compute_tangents(&mut verts, &indices); // must not panic
}

#[test]
fn compute_tangents_degenerate_uv_det_is_skipped() {
    // All UVs identical → determinant = 0, triangle should be skipped gracefully.
    let mut verts: Vec<Vertex> = (0..3)
        .map(|i| Vertex {
            position: [i as f32, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.5, 0.5], // all same → det = 0
            tangent: [0.0, 0.0, 0.0],
            bitangent: [0.0, 0.0, 0.0],
            _pad: [0.0, 0.0],
        })
        .collect();
    let indices = vec![0u32, 1, 2];
    compute_tangents(&mut verts, &indices); // must not panic or produce NaN
    for v in &verts {
        assert!(!v.tangent[0].is_nan() && !v.tangent[1].is_nan() && !v.tangent[2].is_nan());
    }
}

// ── Cube mesh: tangent generation ────────────────────────────────────────────

#[test]
fn compute_tangents_cube_produces_nonzero_tangents() {
    let mut mesh = generate_cube();
    compute_tangents(&mut mesh.vertices, &mesh.indices);

    let any_nonzero = mesh
        .vertices
        .iter()
        .any(|v| length3(v.tangent) > 1e-6);
    assert!(any_nonzero, "compute_tangents must produce non-zero tangents on cube");
}

#[test]
fn compute_tangents_cube_produces_nonzero_bitangents() {
    let mut mesh = generate_cube();
    compute_tangents(&mut mesh.vertices, &mesh.indices);

    let any_nonzero = mesh
        .vertices
        .iter()
        .any(|v| length3(v.bitangent) > 1e-6);
    assert!(any_nonzero, "compute_tangents must produce non-zero bitangents on cube");
}

/// After compute_tangents, tangent must be approximately orthogonal to normal
/// for vertices that received a valid tangent.
#[test]
fn compute_tangents_cube_tangent_orthogonal_to_normal() {
    let mut mesh = generate_cube();
    compute_tangents(&mut mesh.vertices, &mesh.indices);

    for (i, v) in mesh.vertices.iter().enumerate() {
        let t_len = length3(v.tangent);
        if t_len < 1e-6 {
            continue; // skip degenerate vertices
        }
        let dot = dot3(v.normal, v.tangent).abs();
        assert!(
            dot < 0.1,
            "vertex {i}: tangent not orthogonal to normal (dot={dot:.4})"
        );
    }
}

/// After compute_tangents, bitangent must be approximately orthogonal to normal.
#[test]
fn compute_tangents_cube_bitangent_orthogonal_to_normal() {
    let mut mesh = generate_cube();
    compute_tangents(&mut mesh.vertices, &mesh.indices);

    for (i, v) in mesh.vertices.iter().enumerate() {
        let b_len = length3(v.bitangent);
        if b_len < 1e-6 {
            continue;
        }
        let dot = dot3(v.normal, v.bitangent).abs();
        assert!(
            dot < 0.1,
            "vertex {i}: bitangent not orthogonal to normal (dot={dot:.4})"
        );
    }
}

/// Calling compute_tangents twice produces the same result (idempotent output).
#[test]
fn compute_tangents_idempotent() {
    let mut mesh = generate_cube();
    compute_tangents(&mut mesh.vertices, &mesh.indices);

    let tangents_first: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| v.tangent).collect();

    compute_tangents(&mut mesh.vertices, &mesh.indices);

    for (i, (v, t)) in mesh.vertices.iter().zip(tangents_first.iter()).enumerate() {
        let diff = length3([v.tangent[0] - t[0], v.tangent[1] - t[1], v.tangent[2] - t[2]]);
        assert!(
            diff < 0.01,
            "vertex {i}: tangent differs after second compute_tangents call (diff={diff:.6})"
        );
    }
}

// ── Simple hand-crafted triangle: known tangent direction ─────────────────────

/// A flat quad in the XY plane with aligned UVs should produce tangent ≈ +X.
#[test]
fn compute_tangents_flat_quad_tangent_is_plus_x() {
    // Quad in XY plane, normal +Z, UV aligned: u → +X, v → +Y.
    let mut verts = vec![
        Vertex {
            position: [0.0, 0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
            tangent: [0.0; 3],
            bitangent: [0.0; 3],
            _pad: [0.0; 2],
        },
        Vertex {
            position: [1.0, 0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
            tangent: [0.0; 3],
            bitangent: [0.0; 3],
            _pad: [0.0; 2],
        },
        Vertex {
            position: [1.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
            tangent: [0.0; 3],
            bitangent: [0.0; 3],
            _pad: [0.0; 2],
        },
        Vertex {
            position: [0.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
            tangent: [0.0; 3],
            bitangent: [0.0; 3],
            _pad: [0.0; 2],
        },
    ];
    let indices = vec![0u32, 1, 2, 0, 2, 3];
    compute_tangents(&mut verts, &indices);

    for (i, v) in verts.iter().enumerate() {
        let t_len = length3(v.tangent);
        assert!(t_len > 0.5, "vertex {i}: tangent should be non-zero");
        // Tangent should be roughly +X
        assert!(
            v.tangent[0] > 0.5,
            "vertex {i}: tangent.x={} should be ~1.0 (aligned with +X UV direction)",
            v.tangent[0]
        );
    }
}
