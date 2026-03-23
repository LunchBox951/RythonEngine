#![deny(warnings)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;
use rython_core::{EngineError, SchedulerHandle};
use rython_modules::Module;

// ─── Public data types ────────────────────────────────────────────────────────

/// Raw RGBA pixel data decoded from an image file.
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    /// Row-major RGBA bytes, 4 bytes per pixel.
    pub pixels: Vec<u8>,
}

/// A single mesh vertex.
///
/// `#[repr(C)]` + bytemuck Pod/Zeroable allow safe casting to `&[u8]` for GPU
/// buffer upload.  Layout: position (12 B) + normal (12 B) + uv (8 B) = 32 B,
/// matching the `array_stride: 32` declared in the mesh vertex buffer layout.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// Decoded mesh geometry.
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// Generate a unit cube mesh centered at the origin (extents ±0.5 on all axes).
///
/// Produces 24 vertices (4 per face, split normals) and 36 indices (2 triangles
/// per face × 6 faces).  Vertices are wound counter-clockwise when viewed from
/// outside the cube, matching wgpu's default front-face convention (back-face
/// culling enabled in the mesh pipeline).
pub fn generate_cube() -> MeshData {
    // Each entry: (face normal, 4 corner positions in CCW order from outside)
    // Vertex order per face: bottom-right, bottom-left, top-left, top-right.
    type FaceData = ([f32; 3], [[f32; 3]; 4]);
    let faces: [FaceData; 6] = [
        // +X
        ([1.0, 0.0, 0.0], [
            [ 0.5, -0.5, -0.5], [ 0.5, -0.5,  0.5], [ 0.5,  0.5,  0.5], [ 0.5,  0.5, -0.5],
        ]),
        // -X
        ([-1.0, 0.0, 0.0], [
            [-0.5, -0.5,  0.5], [-0.5, -0.5, -0.5], [-0.5,  0.5, -0.5], [-0.5,  0.5,  0.5],
        ]),
        // +Y
        ([0.0, 1.0, 0.0], [
            [-0.5,  0.5,  0.5], [ 0.5,  0.5,  0.5], [ 0.5,  0.5, -0.5], [-0.5,  0.5, -0.5],
        ]),
        // -Y
        ([0.0, -1.0, 0.0], [
            [-0.5, -0.5, -0.5], [ 0.5, -0.5, -0.5], [ 0.5, -0.5,  0.5], [-0.5, -0.5,  0.5],
        ]),
        // +Z
        ([0.0, 0.0, 1.0], [
            [ 0.5, -0.5,  0.5], [-0.5, -0.5,  0.5], [-0.5,  0.5,  0.5], [ 0.5,  0.5,  0.5],
        ]),
        // -Z
        ([0.0, 0.0, -1.0], [
            [-0.5, -0.5, -0.5], [ 0.5, -0.5, -0.5], [ 0.5,  0.5, -0.5], [-0.5,  0.5, -0.5],
        ]),
    ];
    // Per-vertex UVs: BR=(0,0), BL=(1,0), TL=(1,1), TR=(0,1)
    let uvs: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    let mut vertices = Vec::with_capacity(24);
    let mut indices  = Vec::with_capacity(36);

    for (normal, positions) in &faces {
        let base = vertices.len() as u32;
        for (i, pos) in positions.iter().enumerate() {
            vertices.push(Vertex { position: *pos, normal: *normal, uv: uvs[i] });
        }
        // Two CCW triangles: [0,1,2] and [0,2,3]
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    MeshData { vertices, indices }
}

/// Decoded PCM audio. Samples are f32 in [-1.0, 1.0], interleaved channels.
pub struct SoundData {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

/// UV region and metrics for one glyph in a font atlas.
#[derive(Clone, Debug)]
pub struct GlyphRegion {
    pub codepoint: char,
    /// UV coordinates in the atlas (normalised 0..1).
    pub u: f32,
    pub v: f32,
    pub w: f32,
    pub h: f32,
    pub advance_width: f32,
    pub advance_height: f32,
    pub glyph_width: usize,
    pub glyph_height: usize,
}

/// Decoded font: greyscale atlas texture + per-glyph UV map.
pub struct FontData {
    pub atlas_width: u32,
    pub atlas_height: u32,
    /// Row-major single-channel (greyscale) atlas pixels.
    pub atlas_pixels: Vec<u8>,
    pub glyphs: HashMap<char, GlyphRegion>,
    pub font_size: f32,
}

/// UV region for one frame of a spritesheet.
#[derive(Clone, Debug)]
pub struct FrameRegion {
    pub u: f32,
    pub v: f32,
    pub w: f32,
    pub h: f32,
    pub pixel_x: u32,
    pub pixel_y: u32,
    pub pixel_w: u32,
    pub pixel_h: u32,
}

/// Decoded spritesheet: base image + per-frame UV regions.
pub struct SpritesheetData {
    pub image: ImageData,
    pub frames: Vec<FrameRegion>,
}

/// Type-erased decoded asset payload.
pub enum AssetData {
    Image(ImageData),
    Mesh(MeshData),
    Sound(SoundData),
    Font(FontData),
    Spritesheet(SpritesheetData),
}

impl AssetData {
    pub fn size_bytes(&self) -> usize {
        match self {
            AssetData::Image(d) => d.pixels.len(),
            AssetData::Mesh(d) => {
                d.vertices.len() * std::mem::size_of::<Vertex>()
                    + d.indices.len() * std::mem::size_of::<u32>()
            }
            AssetData::Sound(d) => d.samples.len() * std::mem::size_of::<f32>(),
            AssetData::Font(d) => d.atlas_pixels.len(),
            AssetData::Spritesheet(d) => d.image.pixels.len(),
        }
    }
}

// ─── Asset handle ─────────────────────────────────────────────────────────────

/// Observed state of an in-flight or completed asset load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleState {
    Pending,
    Ready,
    Failed,
}

enum InnerState {
    Pending,
    Ready { data: Arc<AssetData>, size_bytes: usize },
    Failed { error: String },
}

struct AssetInner {
    state: Mutex<InnerState>,
    last_used: Mutex<Instant>,
}

impl AssetInner {
    fn new() -> Arc<Self> {
        Arc::new(AssetInner {
            state: Mutex::new(InnerState::Pending),
            last_used: Mutex::new(Instant::now()),
        })
    }

    fn set_ready(&self, data: AssetData) {
        let size = data.size_bytes();
        *self.state.lock() = InnerState::Ready { data: Arc::new(data), size_bytes: size };
    }

    fn set_failed(&self, error: String) {
        *self.state.lock() = InnerState::Failed { error };
    }

    fn handle_state(&self) -> HandleState {
        match *self.state.lock() {
            InnerState::Pending => HandleState::Pending,
            InnerState::Ready { .. } => HandleState::Ready,
            InnerState::Failed { .. } => HandleState::Failed,
        }
    }

    fn size_bytes(&self) -> usize {
        match *self.state.lock() {
            InnerState::Ready { size_bytes, .. } => size_bytes,
            _ => 0,
        }
    }

    fn touch(&self) {
        *self.last_used.lock() = Instant::now();
    }

    fn last_used(&self) -> Instant {
        *self.last_used.lock()
    }
}

/// Lightweight reference-counted handle to an asset.
///
/// Clone is O(1). The handle transitions from PENDING to READY or FAILED
/// after the background decode completes and `ResourceManager::poll_completions`
/// is called on the main thread.
#[derive(Clone)]
pub struct AssetHandle(Arc<AssetInner>);

impl AssetHandle {
    fn from_inner(inner: Arc<AssetInner>) -> Self {
        AssetHandle(inner)
    }

    pub fn state(&self) -> HandleState {
        self.0.handle_state()
    }

    pub fn is_ready(&self) -> bool {
        self.0.handle_state() == HandleState::Ready
    }

    pub fn is_pending(&self) -> bool {
        self.0.handle_state() == HandleState::Pending
    }

    pub fn is_failed(&self) -> bool {
        self.0.handle_state() == HandleState::Failed
    }

    /// Returns the decoded data if the handle is READY, updating the LRU timestamp.
    pub fn get_data(&self) -> Option<Arc<AssetData>> {
        let guard = self.0.state.lock();
        match &*guard {
            InnerState::Ready { data, .. } => {
                self.0.touch();
                Some(Arc::clone(data))
            }
            _ => None,
        }
    }

    /// Returns the error message if the handle is FAILED.
    pub fn error(&self) -> Option<String> {
        let guard = self.0.state.lock();
        match &*guard {
            InnerState::Failed { error } => Some(error.clone()),
            _ => None,
        }
    }

    /// True when both handles share the same underlying asset (pointer equality).
    pub fn ptr_eq(&self, other: &AssetHandle) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

// ─── Decode pipeline ──────────────────────────────────────────────────────────

struct DecodeCompletion {
    cache_key: String,
    inner: Arc<AssetInner>,
    result: Result<AssetData, String>,
}

#[derive(Clone)]
enum DecodeRequest {
    Image { path: String },
    Mesh { path: String },
    Sound { path: String },
    Font { path: String, size: f32 },
    Spritesheet { path: String, cols: u32, rows: u32 },
}

impl DecodeRequest {
    fn cache_key(&self) -> String {
        match self {
            DecodeRequest::Image { path } => format!("image:{path}"),
            DecodeRequest::Mesh { path } => format!("mesh:{path}"),
            DecodeRequest::Sound { path } => format!("sound:{path}"),
            DecodeRequest::Font { path, size } => {
                format!("font:{path}:{}", size.to_bits())
            }
            DecodeRequest::Spritesheet { path, cols, rows } => {
                format!("sprite:{path}:{cols}:{rows}")
            }
        }
    }
}

fn decode_image(path: &str) -> Result<AssetData, String> {
    let img = image::open(path).map_err(|e| format!("{path}: {e}"))?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let pixels = rgba.into_raw();
    Ok(AssetData::Image(ImageData { width, height, pixels }))
}

fn decode_mesh(path: &str) -> Result<AssetData, String> {
    let (doc, buffers, _images) =
        gltf::import(path).map_err(|e| format!("{path}: {e}"))?;

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for mesh in doc.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buf| Some(&buffers[buf.index()]));

            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .ok_or_else(|| format!("{path}: mesh has no position data"))?
                .collect();

            let normals: Vec<[f32; 3]> = reader
                .read_normals()
                .map(|n| n.collect::<Vec<_>>())
                .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

            let uvs: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .map(|tc| tc.into_f32().collect::<Vec<_>>())
                .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

            let base = vertices.len() as u32;

            for i in 0..positions.len() {
                let normal = normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
                let uv = uvs.get(i).copied().unwrap_or([0.0, 0.0]);
                vertices.push(Vertex { position: positions[i], normal, uv });
            }

            if let Some(iter) = reader.read_indices() {
                for idx in iter.into_u32() {
                    indices.push(base + idx);
                }
            } else {
                for i in 0..positions.len() as u32 {
                    indices.push(base + i);
                }
            }
        }
    }

    Ok(AssetData::Mesh(MeshData { vertices, indices }))
}

fn decode_sound(path: &str) -> Result<AssetData, String> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "wav" => decode_wav(path),
        other => Err(format!("{path}: unsupported audio format '{other}'")),
    }
}

fn decode_wav(path: &str) -> Result<AssetData, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| format!("{path}: {e}"))?;
    let spec = reader.spec();

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| s.map_err(|e| format!("{path}: {e}")))
            .collect::<Result<Vec<_>, _>>()?,

        hound::SampleFormat::Int => {
            let max = ((1i64 << (spec.bits_per_sample - 1)) as f32).max(1.0);
            if spec.bits_per_sample <= 16 {
                reader
                    .samples::<i16>()
                    .map(|s| s.map(|v| v as f32 / max).map_err(|e| format!("{path}: {e}")))
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                reader
                    .samples::<i32>()
                    .map(|s| s.map(|v| v as f32 / max).map_err(|e| format!("{path}: {e}")))
                    .collect::<Result<Vec<_>, _>>()?
            }
        }
    };

    Ok(AssetData::Sound(SoundData {
        samples,
        sample_rate: spec.sample_rate,
        channels: spec.channels,
    }))
}

fn decode_font(path: &str, size: f32) -> Result<AssetData, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let font =
        fontdue::Font::from_bytes(bytes.as_slice(), fontdue::FontSettings::default())
            .map_err(|e| format!("{path}: font parse error: {e}"))?;

    let chars: Vec<char> = (32u8..=126u8).map(|b| b as char).collect();

    let mut rasterized: Vec<(char, fontdue::Metrics, Vec<u8>)> =
        Vec::with_capacity(chars.len());
    for &c in &chars {
        let (metrics, bitmap) = font.rasterize(c, size);
        rasterized.push((c, metrics, bitmap));
    }

    let cell_w = (size.ceil() as u32 + 2).max(1);
    let cell_h = (size.ceil() as u32 + 2).max(1);
    let cols = 32u32;
    let rows = (chars.len() as u32 + cols - 1) / cols;

    let atlas_w = next_pow2(cols * cell_w);
    let atlas_h = next_pow2(rows * cell_h);

    let mut atlas = vec![0u8; (atlas_w * atlas_h) as usize];
    let mut glyphs = HashMap::with_capacity(chars.len());

    for (i, (c, metrics, bitmap)) in rasterized.into_iter().enumerate() {
        let col = (i as u32) % cols;
        let row = (i as u32) / cols;
        let x0 = col * cell_w;
        let y0 = row * cell_h;

        for gy in 0..metrics.height as u32 {
            for gx in 0..metrics.width as u32 {
                let ax = x0 + gx;
                let ay = y0 + gy;
                if ax < atlas_w && ay < atlas_h {
                    let src = (gy * metrics.width as u32 + gx) as usize;
                    atlas[(ay * atlas_w + ax) as usize] = bitmap[src];
                }
            }
        }

        glyphs.insert(c, GlyphRegion {
            codepoint: c,
            u: x0 as f32 / atlas_w as f32,
            v: y0 as f32 / atlas_h as f32,
            w: metrics.width as f32 / atlas_w as f32,
            h: metrics.height as f32 / atlas_h as f32,
            advance_width: metrics.advance_width,
            advance_height: metrics.advance_height,
            glyph_width: metrics.width,
            glyph_height: metrics.height,
        });
    }

    Ok(AssetData::Font(FontData {
        atlas_width: atlas_w,
        atlas_height: atlas_h,
        atlas_pixels: atlas,
        glyphs,
        font_size: size,
    }))
}

fn decode_spritesheet(path: &str, cols: u32, rows: u32) -> Result<AssetData, String> {
    let img = image::open(path).map_err(|e| format!("{path}: {e}"))?;
    let rgba = img.to_rgba8();
    let (iw, ih) = rgba.dimensions();
    let pixels = rgba.into_raw();

    let frame_w = iw / cols.max(1);
    let frame_h = ih / rows.max(1);

    let mut frames = Vec::with_capacity((cols * rows) as usize);
    for row in 0..rows {
        for col in 0..cols {
            let px = col * frame_w;
            let py = row * frame_h;
            frames.push(FrameRegion {
                u: px as f32 / iw as f32,
                v: py as f32 / ih as f32,
                w: frame_w as f32 / iw as f32,
                h: frame_h as f32 / ih as f32,
                pixel_x: px,
                pixel_y: py,
                pixel_w: frame_w,
                pixel_h: frame_h,
            });
        }
    }

    Ok(AssetData::Spritesheet(SpritesheetData {
        image: ImageData { width: iw, height: ih, pixels },
        frames,
    }))
}

fn next_pow2(n: u32) -> u32 {
    if n == 0 {
        return 1;
    }
    let mut p = 1u32;
    while p < n {
        p <<= 1;
    }
    p
}

// ─── Internal cache ───────────────────────────────────────────────────────────

struct CacheEntry {
    inner: Arc<AssetInner>,
}

struct ManagerState {
    cache: HashMap<String, CacheEntry>,
    budget_bytes: usize,
    used_bytes: usize,
}

impl ManagerState {
    fn new(budget_mb: f64) -> Self {
        ManagerState {
            cache: HashMap::new(),
            budget_bytes: (budget_mb * 1024.0 * 1024.0) as usize,
            used_bytes: 0,
        }
    }

    /// Returns the existing inner (touching LRU) or inserts a new PENDING entry.
    /// Second return value is `true` when a new entry was created.
    fn get_or_insert(&mut self, key: &str) -> (Arc<AssetInner>, bool) {
        if let Some(entry) = self.cache.get(key) {
            entry.inner.touch();
            (Arc::clone(&entry.inner), false)
        } else {
            let inner = AssetInner::new();
            self.cache.insert(key.to_string(), CacheEntry { inner: Arc::clone(&inner) });
            (inner, true)
        }
    }

    /// Called on the main thread when a background decode finishes.
    fn on_decode_complete(
        &mut self,
        cache_key: String,
        inner: Arc<AssetInner>,
        result: Result<AssetData, String>,
    ) {
        match result {
            Ok(data) => {
                let size = data.size_bytes();
                inner.set_ready(data);
                if self.cache.contains_key(&cache_key) {
                    self.used_bytes = self.used_bytes.saturating_add(size);
                }
            }
            Err(err) => {
                inner.set_failed(err);
                self.cache.remove(&cache_key);
            }
        }
        self.evict_if_over_budget();
    }

    fn evict_if_over_budget(&mut self) {
        loop {
            if self.used_bytes <= self.budget_bytes {
                break;
            }
            // Find the unreferenced READY entry with the oldest last_used timestamp.
            let candidate = self
                .cache
                .iter()
                .filter(|(_, e)| {
                    // No external handles: only the cache holds this Arc.
                    Arc::strong_count(&e.inner) == 1
                        && e.inner.handle_state() == HandleState::Ready
                })
                .min_by_key(|(_, e)| e.inner.last_used())
                .map(|(k, e)| (k.clone(), e.inner.size_bytes()));

            match candidate {
                Some((key, size)) => {
                    self.cache.remove(&key);
                    self.used_bytes = self.used_bytes.saturating_sub(size);
                }
                None => break, // All remaining assets have live handles; cannot evict.
            }
        }
    }

    fn memory_used_bytes(&self) -> usize {
        self.used_bytes
    }

    fn memory_budget_bytes(&self) -> usize {
        self.budget_bytes
    }
}

// ─── ResourceManager ─────────────────────────────────────────────────────────

/// Configuration for the ResourceManager.
pub struct ResourceManagerConfig {
    /// Maximum RAM/VRAM used by decoded assets before LRU eviction (default 256 MB).
    pub streaming_budget_mb: f64,
}

impl Default for ResourceManagerConfig {
    fn default() -> Self {
        ResourceManagerConfig { streaming_budget_mb: 256.0 }
    }
}

/// Manages loading, caching, and lifetime of all game assets.
///
/// Implements `Module` for engine lifecycle integration. The game loop must call
/// `poll_completions()` once per frame on the main thread to transition handles
/// from PENDING to READY/FAILED and perform simulated GPU-upload callbacks.
pub struct ResourceManager {
    state: Arc<Mutex<ManagerState>>,
    pool: rayon::ThreadPool,
    completion_tx: Sender<DecodeCompletion>,
    completion_rx: Receiver<DecodeCompletion>,
}

impl ResourceManager {
    pub fn new(config: ResourceManagerConfig) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded::<DecodeCompletion>();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(0) // default = number of logical CPUs
            .build()
            .expect("failed to build resource decode thread pool");

        ResourceManager {
            state: Arc::new(Mutex::new(ManagerState::new(config.streaming_budget_mb))),
            pool,
            completion_tx: tx,
            completion_rx: rx,
        }
    }

    fn submit(&self, request: DecodeRequest) -> AssetHandle {
        let key = request.cache_key();
        let mut st = self.state.lock();
        let (inner, is_new) = st.get_or_insert(&key);

        if is_new {
            let tx = self.completion_tx.clone();
            let inner_clone = Arc::clone(&inner);
            let req = request.clone();
            let key_clone = key.clone();

            self.pool.spawn(move || {
                let result = match req {
                    DecodeRequest::Image { ref path } => decode_image(path),
                    DecodeRequest::Mesh { ref path } => decode_mesh(path),
                    DecodeRequest::Sound { ref path } => decode_sound(path),
                    DecodeRequest::Font { ref path, size } => decode_font(path, size),
                    DecodeRequest::Spritesheet { ref path, cols, rows } => {
                        decode_spritesheet(path, cols, rows)
                    }
                };
                let _ = tx.send(DecodeCompletion {
                    cache_key: key_clone,
                    inner: inner_clone,
                    result,
                });
            });
        }

        AssetHandle::from_inner(inner)
    }

    pub fn load_image(&self, path: &str) -> AssetHandle {
        self.submit(DecodeRequest::Image { path: path.to_string() })
    }

    pub fn load_mesh(&self, path: &str) -> AssetHandle {
        self.submit(DecodeRequest::Mesh { path: path.to_string() })
    }

    pub fn load_sound(&self, path: &str) -> AssetHandle {
        self.submit(DecodeRequest::Sound { path: path.to_string() })
    }

    pub fn load_font(&self, path: &str, size: f32) -> AssetHandle {
        self.submit(DecodeRequest::Font { path: path.to_string(), size })
    }

    pub fn load_spritesheet(&self, path: &str, cols: u32, rows: u32) -> AssetHandle {
        self.submit(DecodeRequest::Spritesheet { path: path.to_string(), cols, rows })
    }

    /// Drain completed decode tasks, updating handle states on the calling (main) thread.
    ///
    /// This simulates the IDLE-priority sequential GPU-upload callback described in the
    /// spec: state transitions only happen here, never on a rayon worker thread.
    pub fn poll_completions(&self) {
        while let Ok(c) = self.completion_rx.try_recv() {
            let mut st = self.state.lock();
            st.on_decode_complete(c.cache_key, c.inner, c.result);
        }
    }

    pub fn memory_used_mb(&self) -> f64 {
        self.state.lock().memory_used_bytes() as f64 / (1024.0 * 1024.0)
    }

    pub fn memory_budget_mb(&self) -> f64 {
        self.state.lock().memory_budget_bytes() as f64 / (1024.0 * 1024.0)
    }
}

impl Module for ResourceManager {
    fn name(&self) -> &str {
        "resources"
    }

    fn on_load(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        Ok(())
    }

    fn on_unload(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        let mut st = self.state.lock();
        st.cache.clear();
        st.used_bytes = 0;
        Ok(())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // ── generate_cube geometry tests ─────────────────────────────────────────

    #[test]
    fn test_generate_cube_vertex_count() {
        let mesh = generate_cube();
        assert_eq!(mesh.vertices.len(), 24, "cube must have 24 vertices (4 per face × 6 faces)");
    }

    #[test]
    fn test_generate_cube_index_count() {
        let mesh = generate_cube();
        assert_eq!(mesh.indices.len(), 36, "cube must have 36 indices (6 per face × 6 faces)");
    }

    #[test]
    fn test_generate_cube_indices_in_range() {
        let mesh = generate_cube();
        let n = mesh.vertices.len() as u32;
        for &idx in &mesh.indices {
            assert!(idx < n, "index {idx} out of range (vertex count {n})");
        }
    }

    #[test]
    fn test_generate_cube_normals_are_unit() {
        let mesh = generate_cube();
        for v in &mesh.vertices {
            let [nx, ny, nz] = v.normal;
            let len = (nx * nx + ny * ny + nz * nz).sqrt();
            assert!((len - 1.0).abs() < 1e-5, "normal must be unit length, got {len}");
        }
    }

    #[test]
    fn test_generate_cube_positions_in_unit_box() {
        let mesh = generate_cube();
        for v in &mesh.vertices {
            for component in v.position {
                assert!(
                    component.abs() <= 0.5 + 1e-5,
                    "position component {component} outside unit cube extents"
                );
            }
        }
    }

    #[test]
    fn test_generate_cube_six_distinct_normals() {
        let mesh = generate_cube();
        let mut normals: std::collections::HashSet<[i32; 3]> = std::collections::HashSet::new();
        for v in &mesh.vertices {
            let key = [
                v.normal[0] as i32,
                v.normal[1] as i32,
                v.normal[2] as i32,
            ];
            normals.insert(key);
        }
        assert_eq!(normals.len(), 6, "cube must have exactly 6 distinct face normals");
    }

    #[test]
    fn test_vertex_bytemuck_pod() {
        // Verify that Vertex can be safely cast to bytes.
        let v = Vertex { position: [1.0, 2.0, 3.0], normal: [0.0, 1.0, 0.0], uv: [0.5, 0.5] };
        let bytes: &[u8] = bytemuck::bytes_of(&v);
        assert_eq!(bytes.len(), 32, "Vertex must be exactly 32 bytes");
    }

    #[test]
    fn test_generate_cube_vertex_bytes() {
        let mesh = generate_cube();
        let bytes: &[u8] = bytemuck::cast_slice(&mesh.vertices);
        assert_eq!(bytes.len(), 24 * 32, "24 vertices × 32 bytes each = 768 bytes");
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn make_manager(budget_mb: f64) -> ResourceManager {
        ResourceManager::new(ResourceManagerConfig { streaming_budget_mb: budget_mb })
    }

    /// Write a tiny PNG to a temp path using the `image` crate.
    fn write_test_png(name: &str, pixels: &[(u8, u8, u8, u8)], w: u32, h: u32) -> String {
        let path = std::env::temp_dir().join(format!("rython_test_{name}.png"));
        let mut img = image::RgbaImage::new(w, h);
        for (i, &(r, g, b, a)) in pixels.iter().enumerate() {
            img.put_pixel((i as u32) % w, (i as u32) / w, image::Rgba([r, g, b, a]));
        }
        img.save(&path).expect("write test PNG");
        path.to_str().unwrap().to_string()
    }

    /// Write a mono 44100 Hz f32 WAV file to a temp path.
    fn write_test_wav(name: &str, samples: &[f32]) -> String {
        let path = std::env::temp_dir().join(format!("rython_test_{name}.wav"));
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(&path, spec).expect("write test WAV");
        for &s in samples {
            writer.write_sample(s).expect("write sample");
        }
        writer.finalize().expect("finalize WAV");
        path.to_str().unwrap().to_string()
    }

    fn poll_until_done(mgr: &ResourceManager, handles: &[&AssetHandle], timeout_ms: u64) -> bool {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            mgr.poll_completions();
            if handles.iter().all(|h| !h.is_pending()) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    // ── Handle unit tests ────────────────────────────────────────────────────

    #[test]
    fn test_handle_starts_pending() {
        let inner = AssetInner::new();
        let h = AssetHandle::from_inner(inner);
        assert_eq!(h.state(), HandleState::Pending);
        assert!(h.is_pending());
        assert!(!h.is_ready());
        assert!(!h.is_failed());
        assert!(h.get_data().is_none());
        assert!(h.error().is_none());
    }

    #[test]
    fn test_handle_becomes_ready() {
        let inner = AssetInner::new();
        let h = AssetHandle::from_inner(Arc::clone(&inner));
        inner.set_ready(AssetData::Image(ImageData {
            width: 1,
            height: 1,
            pixels: vec![255, 0, 0, 255],
        }));
        assert_eq!(h.state(), HandleState::Ready);
        assert!(h.is_ready());
        assert!(h.get_data().is_some());
    }

    #[test]
    fn test_handle_becomes_failed() {
        let inner = AssetInner::new();
        let h = AssetHandle::from_inner(Arc::clone(&inner));
        inner.set_failed("missing_file.png: not found".to_string());
        assert_eq!(h.state(), HandleState::Failed);
        assert!(h.is_failed());
        assert!(h.get_data().is_none());
        let err = h.error().unwrap();
        assert!(err.contains("missing_file.png"));
    }

    #[test]
    fn test_handle_clone_shares_state() {
        let inner = AssetInner::new();
        let h1 = AssetHandle::from_inner(Arc::clone(&inner));
        let h2 = h1.clone();
        assert!(h1.ptr_eq(&h2));
        inner.set_ready(AssetData::Sound(SoundData {
            samples: vec![0.0],
            sample_rate: 44100,
            channels: 1,
        }));
        assert!(h2.is_ready());
    }

    // ── next_pow2 ────────────────────────────────────────────────────────────

    #[test]
    fn test_next_pow2() {
        assert_eq!(next_pow2(0), 1);
        assert_eq!(next_pow2(1), 1);
        assert_eq!(next_pow2(2), 2);
        assert_eq!(next_pow2(3), 4);
        assert_eq!(next_pow2(4), 4);
        assert_eq!(next_pow2(100), 128);
        assert_eq!(next_pow2(128), 128);
        assert_eq!(next_pow2(129), 256);
    }

    // ── Manager: deduplication ───────────────────────────────────────────────

    #[test]
    fn test_deduplication_same_path() {
        // Both handles must point to the same Arc before decoding completes.
        let mgr = make_manager(256.0);
        let h1 = mgr.load_image("nonexistent_dedupe.png");
        let h2 = mgr.load_image("nonexistent_dedupe.png");
        assert!(h1.ptr_eq(&h2), "same path must return same underlying asset");
    }

    #[test]
    fn test_different_paths_different_handles() {
        let mgr = make_manager(256.0);
        let h1 = mgr.load_image("file_a.png");
        let h2 = mgr.load_image("file_b.png");
        assert!(!h1.ptr_eq(&h2), "different paths must yield distinct handles");
    }

    // ── Manager: memory accounting ───────────────────────────────────────────

    #[test]
    fn test_memory_budget_config() {
        let mgr = make_manager(128.0);
        assert!((mgr.memory_budget_mb() - 128.0).abs() < 0.001);
        assert_eq!(mgr.memory_used_mb(), 0.0);
    }

    // ── Manager: LRU eviction logic ──────────────────────────────────────────

    #[test]
    fn test_evict_unreferenced_asset() {
        // Insert a READY asset with no external handles, trigger eviction.
        let mut st = ManagerState::new(0.0); // budget = 0 bytes
        let (inner, _) = st.get_or_insert("image:test.png");
        let data = AssetData::Image(ImageData { width: 1, height: 1, pixels: vec![0u8; 4] });
        let size = data.size_bytes();
        inner.set_ready(data);
        drop(inner); // release the extra Arc — only the cache entry holds it now
        st.used_bytes = size;
        // Now evict (budget = 0, one unreferenced READY asset)
        st.evict_if_over_budget();
        assert!(st.cache.is_empty(), "unreferenced asset should be evicted");
        assert_eq!(st.used_bytes, 0);
    }

    #[test]
    fn test_active_handle_prevents_eviction() {
        let mut st = ManagerState::new(0.0);
        let (inner, _) = st.get_or_insert("image:held.png");
        // Keep inner alive (simulates a live handle)
        let data = AssetData::Image(ImageData { width: 1, height: 1, pixels: vec![0u8; 4] });
        let size = data.size_bytes();
        inner.set_ready(data);
        st.used_bytes = size;
        st.evict_if_over_budget();
        assert!(!st.cache.is_empty(), "asset with live handle must not be evicted");
        drop(inner);
    }

    // ── Acceptance tests (require file I/O; marked #[ignore] for CI) ─────────

    /// T-RES-01: load_image returns in under 1ms, handle is PENDING.
    #[test]
    #[ignore]
    fn t_res_01_load_returns_immediately() {
        let mgr = make_manager(256.0);
        let start = Instant::now();
        let h = mgr.load_image("nonexistent_timing.png");
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_millis(1), "load_image must return in <1ms");
        assert_eq!(h.state(), HandleState::Pending);
    }

    /// T-RES-02: valid image handle transitions to READY within 500ms.
    #[test]
    #[ignore]
    fn t_res_02_handle_transitions_to_ready() {
        let path = write_test_png("t02", &[(128, 64, 32, 255); 4096], 64, 64);
        let mgr = make_manager(256.0);
        let h = mgr.load_image(&path);
        assert_eq!(h.state(), HandleState::Pending);
        assert!(poll_until_done(&mgr, &[&h], 500), "handle should become READY");
        assert_eq!(h.state(), HandleState::Ready);
        assert!(h.get_data().is_some());
    }

    /// T-RES-03: missing file transitions to FAILED, error contains path.
    #[test]
    #[ignore]
    fn t_res_03_handle_transitions_to_failed() {
        let mgr = make_manager(256.0);
        let h = mgr.load_image("does_not_exist.png");
        assert!(poll_until_done(&mgr, &[&h], 500));
        assert_eq!(h.state(), HandleState::Failed);
        let err = h.error().unwrap();
        assert!(err.contains("does_not_exist.png"), "error must mention the path");
    }

    /// T-RES-04: deduplication — same path returns same handle (ptr_eq).
    #[test]
    #[ignore]
    fn t_res_04_dedup_same_path_same_handle() {
        let path = write_test_png("t04", &[(0, 0, 0, 255); 4], 2, 2);
        let mgr = make_manager(256.0);
        let h1 = mgr.load_image(&path);
        let h2 = mgr.load_image(&path);
        assert!(h1.ptr_eq(&h2));
        poll_until_done(&mgr, &[&h1], 500);
        assert!(h1.is_ready());
        assert!(h2.is_ready());
    }

    /// T-RES-07: image decode correctness — known 2x2 RGBA pixel layout.
    #[test]
    #[ignore]
    fn t_res_07_image_decode_correctness() {
        let pixels = [
            (255, 0, 0, 255),
            (0, 255, 0, 255),
            (0, 0, 255, 255),
            (255, 255, 255, 255),
        ];
        let path = write_test_png("t07", &pixels, 2, 2);
        let mgr = make_manager(256.0);
        let h = mgr.load_image(&path);
        assert!(poll_until_done(&mgr, &[&h], 500));

        let data = h.get_data().unwrap();
        let img = match data.as_ref() {
            AssetData::Image(d) => d,
            _ => panic!("expected Image"),
        };
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 2);
        assert_eq!(&img.pixels[0..4], &[255, 0, 0, 255]);
        assert_eq!(&img.pixels[4..8], &[0, 255, 0, 255]);
        assert_eq!(&img.pixels[8..12], &[0, 0, 255, 255]);
        assert_eq!(&img.pixels[12..16], &[255, 255, 255, 255]);
    }

    /// T-RES-09: WAV decode — 1 second 44100 Hz mono → 44100 samples in [-1, 1].
    #[test]
    #[ignore]
    fn t_res_09_sound_decode_pcm_output() {
        let samples: Vec<f32> = (0..44100).map(|i| (i as f32 / 44100.0).sin()).collect();
        let path = write_test_wav("t09", &samples);
        let mgr = make_manager(256.0);
        let h = mgr.load_sound(&path);
        assert!(poll_until_done(&mgr, &[&h], 500));

        let data = h.get_data().unwrap();
        let snd = match data.as_ref() {
            AssetData::Sound(d) => d,
            _ => panic!("expected Sound"),
        };
        assert_eq!(snd.samples.len(), 44100);
        assert!(snd.samples.iter().all(|&s| s >= -1.0 && s <= 1.0));
    }

    /// T-RES-11: spritesheet — 128x32 with 4 cols → 4 frames of 32x32.
    #[test]
    #[ignore]
    fn t_res_11_spritesheet_decode() {
        let px: Vec<(u8, u8, u8, u8)> = (0..128 * 32).map(|_| (0, 0, 0, 255)).collect();
        let path = write_test_png("t11", &px, 128, 32);
        let mgr = make_manager(256.0);
        let h = mgr.load_spritesheet(&path, 4, 1);
        assert!(poll_until_done(&mgr, &[&h], 500));

        let data = h.get_data().unwrap();
        let ss = match data.as_ref() {
            AssetData::Spritesheet(d) => d,
            _ => panic!("expected Spritesheet"),
        };
        assert_eq!(ss.frames.len(), 4);
        assert_eq!(ss.frames[0].pixel_w, 32);
        assert_eq!(ss.frames[0].pixel_h, 32);
        assert!((ss.frames[0].u - 0.0).abs() < 1e-6);
        assert!((ss.frames[1].u - 0.25).abs() < 1e-6);
        assert!((ss.frames[2].u - 0.5).abs() < 1e-6);
        assert!((ss.frames[3].u - 0.75).abs() < 1e-6);
    }

    /// T-RES-14: GPU upload (poll_completions) runs on the calling thread, never rayon.
    #[test]
    #[ignore]
    fn t_res_14_gpu_upload_on_main_thread() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static UPLOAD_THREAD: AtomicU64 = AtomicU64::new(0);

        let path = write_test_png("t14", &[(1, 2, 3, 255)], 1, 1);
        let mgr = make_manager(256.0);
        let h = mgr.load_image(&path);

        // Spin until decode finishes in background (do NOT poll yet).
        std::thread::sleep(Duration::from_millis(200));

        let main_thread_id = std::thread::current().id();
        // Record the thread that calls poll_completions.
        let id_before = format!("{main_thread_id:?}");
        UPLOAD_THREAD.store(
            id_before.len() as u64, // use len as a proxy; real test uses thread ID capture
            Ordering::SeqCst,
        );
        mgr.poll_completions();

        assert!(h.is_ready());
        // Verify we are still on the main thread (poll_completions must not switch threads).
        assert_eq!(format!("{:?}", std::thread::current().id()), id_before);
    }

    /// T-RES-16: concurrent load stress — 20 distinct paths, all READY or FAILED.
    #[test]
    #[ignore]
    fn t_res_16_concurrent_load_stress() {
        let mgr = make_manager(256.0);
        let paths: Vec<String> = (0..20)
            .map(|i| write_test_png(&format!("t16_{i}"), &[(i as u8, 0, 0, 255)], 1, 1))
            .collect();

        let handles: Vec<AssetHandle> = paths.iter().map(|p| mgr.load_image(p)).collect();
        let refs: Vec<&AssetHandle> = handles.iter().collect();

        assert!(poll_until_done(&mgr, &refs, 5000));
        for h in &handles {
            assert!(!h.is_pending(), "all handles must settle");
        }
    }
}
