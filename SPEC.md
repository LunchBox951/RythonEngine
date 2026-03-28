High-Impact, Moderate Effort

1. Normal Mapping — Add normal map support to the mesh shader for surface detail without extra geometry. Currently textures are only diffuse color. → [.spec/normal-mapping.spec.md](.spec/normal-mapping.spec.md)
2. Specular Mapping — Split material into diffuse + specular maps. Currently uses a hardcoded shininess field that isn't utilized in the shader. → [.spec/specular-mapping.spec.md](.spec/specular-mapping.spec.md)
3. Shadow Mapping — Render from the light's perspective to generate a shadow map, then use it for dynamic shadows in the main pass. → [.spec/shadow-mapping.spec.md](.spec/shadow-mapping.spec.md)
4. Emissive Materials — Add per-mesh emissive texture/color that self-illuminates without being affected by lighting. → [.spec/emissive-materials.spec.md](.spec/emissive-materials.spec.md)
5. Multiple Light Sources — Extend beyond the single hardcoded directional light at (0.5, 1.0, 0.5) to support point lights and spotlights. → [.spec/multiple-lights.spec.md](.spec/multiple-lights.spec.md)
6. Ambient Occlusion (SSAO) — Screen-space ambient occlusion for darkening crevices and contact shadows—quick post-process. → [.spec/ssao.spec.md](.spec/ssao.spec.md)
7. Post-Processing Pipeline — Add tone mapping, exposure control, and color grading for better visual control. → [.spec/post-processing.spec.md](.spec/post-processing.spec.md)
8. Skybox/Cubemap Reflections — Render a skybox and use it for environment reflections on shiny materials. → [.spec/skybox-cubemap.spec.md](.spec/skybox-cubemap.spec.md)

Medium-Impact, Lower Effort

9. Fog/Atmosphere — Distance-based or height-based fog for depth perception and outdoor environments. → [.spec/fog-atmosphere.spec.md](.spec/fog-atmosphere.spec.md)
10. Rim Lighting — Brighten object silhouettes against the background for visual pop. → [.spec/rim-lighting.spec.md](.spec/rim-lighting.spec.md)
11. Instanced Rendering — Batch similar meshes with different transforms to reduce draw call overhead. → [.spec/instanced-rendering.spec.md](.spec/instanced-rendering.spec.md)
12. LOD (Level of Detail) — Reduce geometry complexity for distant objects; can be toggled in the editor. → [.spec/lod.spec.md](.spec/lod.spec.md)
13. Bloom/HDR — Post-process bloom for bright surfaces; requires HDR render target. → [.spec/bloom-hdr.spec.md](.spec/bloom-hdr.spec.md)
14. Wireframe Mode — Debug visualization; helpful in the editor for inspecting geometry. → [.spec/wireframe-mode.spec.md](.spec/wireframe-mode.spec.md)
15. Alpha Blending/Transparency — Proper depth-sorted rendering of transparent objects (currently not well supported). → [.spec/alpha-blending.spec.md](.spec/alpha-blending.spec.md)

Advanced, Higher Effort

16. Physically Based Rendering (PBR) — Implement metallic/roughness workflow for realistic material properties. → [.spec/pbr.spec.md](.spec/pbr.spec.md)
17. Parallax/Relief Mapping — Advanced normal mapping that creates depth illusion with height parallax. → [.spec/parallax-mapping.spec.md](.spec/parallax-mapping.spec.md)
18. Deferred Rendering — Geometry pass → light pass, enabling many lights efficiently. → [.spec/deferred-rendering.spec.md](.spec/deferred-rendering.spec.md)
19. Screen-Space Reflections (SSR) — Reflect scene geometry on glossy surfaces without cubemaps. → [.spec/ssr.spec.md](.spec/ssr.spec.md)
20. Tangent-Space Normal Compression — Optimize normal map storage and calculation. → [.spec/tangent-space-normals.spec.md](.spec/tangent-space-normals.spec.md)

---
Quick Wins (1-2 hour implementations)

- Fixed-function material properties — Expose metallic/roughness as uniform floats in the mesh component → [.spec/material-properties.spec.md](.spec/material-properties.spec.md#1-fixed-function-material-properties)
- Light direction editor — Make the hardcoded light direction configurable in the editor → [.spec/material-properties.spec.md](.spec/material-properties.spec.md#2-light-direction-editor)
- Background color picker — Currently hardcoded gray (0.15, 0.15, 0.15) → [.spec/material-properties.spec.md](.spec/material-properties.spec.md#3-background-color-picker)
- Depth fog — Linear or exponential fog as a simple post-effect → [.spec/fog-atmosphere.spec.md](.spec/fog-atmosphere.spec.md)


**Acceptance Criteria** - These tests must pass before any commits happen
1. All tests (new and old) pass without fail. `cargo test -q 2>&1`
2. `make build` compiles without errors.

3. When changes to `game/` are made, `make run SCRIPT_DIR=. SCRIPT=game.scripts.main` compiles and runs without errors.
- If changes have been made to visuals, confirm with the user with a screenshot that everything works as intended. (`Screenshot_20260327_123417 (Pre-changes).png` shows how the game looked before changes have been made)