/// Offscreen render target for the 3D viewport.
///
/// Holds a RGBA8UnormSrgb texture that the engine renders into, along with
/// its egui TextureId for display as an egui image.
pub struct ViewportTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub egui_texture_id: egui::TextureId,
    pub width: u32,
    pub height: u32,
}

impl ViewportTexture {
    /// Create a new offscreen texture of the given dimensions, registered
    /// with the egui renderer so it can be displayed in the viewport panel.
    pub fn new(
        device: &wgpu::Device,
        egui_renderer: &mut egui_wgpu::Renderer,
        width: u32,
        height: u32,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewport_offscreen"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let egui_texture_id =
            egui_renderer.register_native_texture(device, &view, wgpu::FilterMode::Linear);
        Self { texture, view, egui_texture_id, width, height }
    }

    /// Recreate the texture if the target dimensions have changed.
    /// Returns true if the texture was recreated.
    pub fn resize_if_needed(
        &mut self,
        device: &wgpu::Device,
        egui_renderer: &mut egui_wgpu::Renderer,
        new_width: u32,
        new_height: u32,
    ) -> bool {
        if self.width == new_width && self.height == new_height {
            return false;
        }
        egui_renderer.free_texture(&self.egui_texture_id);
        *self = Self::new(device, egui_renderer, new_width, new_height);
        true
    }
}
