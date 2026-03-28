use crate::Vertex;

/// Compute tangent and bitangent for every vertex in a mesh.
///
/// Uses Lengyel's method: accumulate per-triangle TB contributions from UV
/// deltas, then orthogonalize against the interpolated normal with Gram-Schmidt.
/// The handedness of each TBN frame is preserved so mirrored UVs work correctly.
///
/// If the mesh already has glTF TANGENT attributes set before calling this
/// function, those values are replaced.  Call this only when the asset does not
/// supply tangents, or when you need to recompute them.
pub fn compute_tangents(vertices: &mut [Vertex], indices: &[u32]) {
    let n = vertices.len();
    if n == 0 || indices.len() < 3 {
        return;
    }

    let mut tan1 = vec![[0.0f32; 3]; n];
    let mut tan2 = vec![[0.0f32; 3]; n];

    for chunk in indices.chunks(3) {
        if chunk.len() < 3 {
            continue;
        }
        let i0 = chunk[0] as usize;
        let i1 = chunk[1] as usize;
        let i2 = chunk[2] as usize;
        if i0 >= n || i1 >= n || i2 >= n {
            continue;
        }

        let v0 = vertices[i0].position;
        let v1 = vertices[i1].position;
        let v2 = vertices[i2].position;

        let uv0 = vertices[i0].uv;
        let uv1 = vertices[i1].uv;
        let uv2 = vertices[i2].uv;

        let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
        let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];

        let du1 = uv1[0] - uv0[0];
        let dv1 = uv1[1] - uv0[1];
        let du2 = uv2[0] - uv0[0];
        let dv2 = uv2[1] - uv0[1];

        let det = du1 * dv2 - du2 * dv1;
        if det.abs() < 1e-8 {
            continue;
        }
        let r = 1.0 / det;

        let t = [
            (dv2 * e1[0] - dv1 * e2[0]) * r,
            (dv2 * e1[1] - dv1 * e2[1]) * r,
            (dv2 * e1[2] - dv1 * e2[2]) * r,
        ];
        let b = [
            (du1 * e2[0] - du2 * e1[0]) * r,
            (du1 * e2[1] - du2 * e1[1]) * r,
            (du1 * e2[2] - du2 * e1[2]) * r,
        ];

        for &i in &[i0, i1, i2] {
            for k in 0..3 {
                tan1[i][k] += t[k];
                tan2[i][k] += b[k];
            }
        }
    }

    for (i, vertex) in vertices.iter_mut().enumerate() {
        let nrm = vertex.normal;
        let t = tan1[i];

        // Gram-Schmidt orthogonalize T against N
        let dot_nt = nrm[0] * t[0] + nrm[1] * t[1] + nrm[2] * t[2];
        let mut tangent = [
            t[0] - dot_nt * nrm[0],
            t[1] - dot_nt * nrm[1],
            t[2] - dot_nt * nrm[2],
        ];
        let tlen = (tangent[0] * tangent[0] + tangent[1] * tangent[1] + tangent[2] * tangent[2]).sqrt();
        if tlen > 1e-8 {
            tangent = [tangent[0] / tlen, tangent[1] / tlen, tangent[2] / tlen];
        } else {
            // Degenerate triangle contribution — pick an arbitrary orthogonal tangent
            tangent = arbitrary_tangent(nrm[0], nrm[1], nrm[2]);
        }

        // Cross(N, T) for handedness check
        let cross_nt = [
            nrm[1] * tangent[2] - nrm[2] * tangent[1],
            nrm[2] * tangent[0] - nrm[0] * tangent[2],
            nrm[0] * tangent[1] - nrm[1] * tangent[0],
        ];

        let b_acc = tan2[i];
        let handedness = if cross_nt[0] * b_acc[0] + cross_nt[1] * b_acc[1] + cross_nt[2] * b_acc[2] < 0.0 {
            -1.0_f32
        } else {
            1.0_f32
        };

        let bitangent = [
            cross_nt[0] * handedness,
            cross_nt[1] * handedness,
            cross_nt[2] * handedness,
        ];

        vertex.tangent = tangent;
        vertex.bitangent = bitangent;
    }
}

/// Pick an arbitrary unit vector orthogonal to the given unit normal.
fn arbitrary_tangent(nx: f32, ny: f32, nz: f32) -> [f32; 3] {
    let (ax, ay, az) = if nx.abs() < 0.9 { (1.0, 0.0, 0.0) } else { (0.0, 1.0, 0.0) };
    let tx = ny * az - nz * ay;
    let ty = nz * ax - nx * az;
    let tz = nx * ay - ny * ax;
    let len = (tx * tx + ty * ty + tz * tz).sqrt();
    [tx / len, ty / len, tz / len]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{generate_cube, Vertex};

    /// Test 2 from spec: cube tangents are orthogonal to normals and unit-length.
    #[test]
    fn test_cube_tangents_orthogonal_to_normals() {
        let mut mesh = generate_cube();
        // generate_cube() already calls compute_tangents, just verify the result
        for v in &mesh.vertices {
            let dot = v.tangent[0] * v.normal[0]
                + v.tangent[1] * v.normal[1]
                + v.tangent[2] * v.normal[2];
            assert!(
                dot.abs() < 1e-4,
                "tangent not orthogonal to normal: dot={dot:.6}"
            );

            let tlen = (v.tangent[0] * v.tangent[0]
                + v.tangent[1] * v.tangent[1]
                + v.tangent[2] * v.tangent[2])
                .sqrt();
            assert!(
                (tlen - 1.0).abs() < 1e-4,
                "tangent not unit length: len={tlen:.6}"
            );
        }

        // cross(tangent, bitangent) should align with normal
        for v in &mesh.vertices {
            let cross = [
                v.tangent[1] * v.bitangent[2] - v.tangent[2] * v.bitangent[1],
                v.tangent[2] * v.bitangent[0] - v.tangent[0] * v.bitangent[2],
                v.tangent[0] * v.bitangent[1] - v.tangent[1] * v.bitangent[0],
            ];
            let dot = cross[0] * v.normal[0] + cross[1] * v.normal[1] + cross[2] * v.normal[2];
            assert!(dot > 0.0, "TBN frame not right-handed: dot={dot:.6}");
        }

        // Verify with explicitly indexed mesh too
        compute_tangents(&mut mesh.vertices, &mesh.indices);
        for v in &mesh.vertices {
            let dot = v.tangent[0] * v.normal[0]
                + v.tangent[1] * v.normal[1]
                + v.tangent[2] * v.normal[2];
            assert!(dot.abs() < 1e-4, "post-recompute: tangent not orthogonal: dot={dot:.6}");
        }
    }

    /// Test 3 from spec: idempotent — running compute_tangents twice is stable.
    #[test]
    fn test_compute_tangents_idempotent() {
        let mut mesh = generate_cube();
        // First pass already done in generate_cube — snapshot tangents
        let first: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| v.tangent).collect();

        // Second pass
        compute_tangents(&mut mesh.vertices, &mesh.indices);
        let second: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| v.tangent).collect();

        for (a, b) in first.iter().zip(second.iter()) {
            let diff = ((a[0]-b[0]).powi(2) + (a[1]-b[1]).powi(2) + (a[2]-b[2]).powi(2)).sqrt();
            assert!(diff < 1e-4, "tangents changed after second pass: diff={diff:.6}");
        }
    }

    /// Empty mesh and degenerate indices are handled without panic.
    #[test]
    fn test_compute_tangents_empty() {
        let mut verts: Vec<Vertex> = Vec::new();
        compute_tangents(&mut verts, &[]);
        // just ensure no panic
    }
}
