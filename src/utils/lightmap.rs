//! Module to generate lightmaps for surfaces.
//!
//! # Performance
//!
//! This is CPU lightmapper, its performance is linear with core count of your CPU.
//!
//! WARNING: There is still work-in-progress, so it is not advised to use lightmapper
//! now!

use crate::{
    core::{
        color::Color,
        math::{self, mat3::Mat3, mat4::Mat4, vec2::Vec2, vec3::Vec3, Rect, TriangleDefinition},
    },
    renderer::{surface::SurfaceSharedData, surface::Vertex},
    resource::texture::{Texture, TextureKind},
};
use std::time;

/// Directional light is a light source with parallel rays. Example: Sun.
pub struct DirectionalLightDefinition {
    /// Intensity is how bright light is. Default is 1.0.
    pub intensity: f32,
    /// Direction of light rays.
    pub direction: Vec3,
    /// Color of light.
    pub color: Color,
}

/// Spot light is a cone light source. Example: flashlight.
pub struct SpotLightDefinition {
    /// Intensity is how bright light is. Default is 1.0.
    pub intensity: f32,
    /// Angle (in radians) at cone top which defines area with uniform light.  
    pub hotspot_cone_angle: f32,
    /// Angle delta (in radians) outside of cone top which sets area of smooth
    /// transition of intensity from max to min.
    pub falloff_angle_delta: f32,
    /// Color of light.
    pub color: Color,
    /// Direction vector of light.
    pub direction: Vec3,
    /// Position of light in world coordinates.
    pub position: Vec3,
    /// Distance at which light intensity decays to zero.
    pub distance: f32,
}

/// Point light is a spherical light source. Example: light bulb.
pub struct PointLightDefinition {
    /// Intensity is how bright light is. Default is 1.0.
    pub intensity: f32,
    /// Position of light in world coordinates.
    pub position: Vec3,
    /// Color of light.
    pub color: Color,
    /// Radius of sphere at which light intensity decays to zero.
    pub radius: f32,
}

/// Light definition for lightmap rendering.
pub enum LightDefinition {
    /// See docs of [DirectionalLightDefinition](struct.PointLightDefinition.html)
    Directional(DirectionalLightDefinition),
    /// See docs of [SpotLightDefinition](struct.SpotLightDefinition.html)
    Spot(SpotLightDefinition),
    /// See docs of [PointLightDefinition](struct.PointLightDefinition.html)
    Point(PointLightDefinition),
}

/// Computes total area of triangles in surface data and returns size of square
/// in which triangles can fit.
fn estimate_size(vertices: &[Vec3], triangles: &[TriangleDefinition], texels_per_unit: u32) -> u32 {
    let mut area = 0.0;
    for triangle in triangles.iter() {
        let a = vertices[triangle[0] as usize];
        let b = vertices[triangle[1] as usize];
        let c = vertices[triangle[2] as usize];
        area += math::triangle_area(a, b, c);
    }
    area.sqrt().ceil() as u32 * texels_per_unit
}

/// Calculates distance attenuation for a point using given distance to the point and
/// radius of a light.
fn distance_attenuation(distance: f32, radius: f32) -> f32 {
    let attenuation = (1.0 - distance * distance / (radius * radius))
        .max(0.0)
        .min(1.0);
    return attenuation * attenuation;
}

/// Transforms vertices of surface data into set of world space positions.
fn transform_vertices(data: &SurfaceSharedData, transform: &Mat4) -> Vec<Vec3> {
    data.vertices
        .iter()
        .map(|v| transform.transform_vector(v.position))
        .collect()
}

enum Pixel {
    Transparent,
    Color {
        color: Color,
        position: Vec3,
        normal: Vec3,
    },
}

/// Calculates properties of pixel (world position, normal) at given position.
fn pick(
    uv: Vec2,
    grid: &Grid,
    triangles: &[TriangleDefinition],
    vertices: &[Vertex],
    world_positions: &[Vec3],
    normal_matrix: &Mat3,
) -> Option<(Vec3, Vec3)> {
    if let Some(cell) = grid.pick(uv) {
        for triangle in cell.triangles.iter().map(|&ti| &triangles[ti]) {
            let uv_a = vertices[triangle[0] as usize].second_tex_coord;
            let uv_b = vertices[triangle[1] as usize].second_tex_coord;
            let uv_c = vertices[triangle[2] as usize].second_tex_coord;

            let barycentric = math::get_barycentric_coords_2d(uv, uv_a, uv_b, uv_c);

            if math::barycentric_is_inside(barycentric) {
                let a = world_positions[triangle[0] as usize];
                let b = world_positions[triangle[1] as usize];
                let c = world_positions[triangle[2] as usize];
                return Some((
                    math::barycentric_to_world(barycentric, a, b, c),
                    normal_matrix.transform_vector(
                        math::barycentric_to_world(
                            barycentric,
                            vertices[triangle[0] as usize].normal,
                            vertices[triangle[1] as usize].normal,
                            vertices[triangle[2] as usize].normal,
                        )
                        .normalized()
                        .unwrap_or(Vec3::UP),
                    ),
                ));
            }
        }
    }
    None
}

struct GridCell {
    // List of triangle indices.
    triangles: Vec<usize>,
}

struct Grid {
    cells: Vec<GridCell>,
    size: usize,
}

impl Grid {
    /// Creates uniform grid where each cell contains list of triangles
    /// whose second texture coordinates intersects with it.
    fn new(data: &SurfaceSharedData, size: usize) -> Self {
        let mut cells = Vec::with_capacity(size);
        let fsize = size as f32;
        for y in 0..size {
            for x in 0..size {
                let bounds = Rect {
                    x: x as f32 / fsize,
                    y: y as f32 / fsize,
                    w: 1.0 / fsize,
                    h: 1.0 / fsize,
                };

                let mut triangles = Vec::new();

                for (triangle_index, triangle) in data.triangles.iter().enumerate() {
                    let uv_a = data.vertices[triangle[0] as usize].second_tex_coord;
                    let uv_b = data.vertices[triangle[1] as usize].second_tex_coord;
                    let uv_c = data.vertices[triangle[2] as usize].second_tex_coord;
                    let uv_min = uv_a.min(uv_b).min(uv_c);
                    let uv_max = uv_a.max(uv_b).max(uv_c);
                    let triangle_bounds = Rect {
                        x: uv_min.x,
                        y: uv_min.y,
                        w: uv_max.x - uv_min.x,
                        h: uv_max.y - uv_min.y,
                    };
                    if triangle_bounds.intersects(bounds) {
                        triangles.push(triangle_index);
                    }
                }

                cells.push(GridCell { triangles })
            }
        }

        Self { cells, size }
    }

    fn pick(&self, v: Vec2) -> Option<&GridCell> {
        let ix = (v.x as f32 * self.size as f32) as usize;
        let iy = (v.y as f32 * self.size as f32) as usize;
        self.cells.get(iy * self.size + ix)
    }
}

/// https://en.wikipedia.org/wiki/Lambert%27s_cosine_law
fn lambertian(light_vec: Vec3, normal: Vec3) -> f32 {
    normal.dot(&light_vec).max(0.0)
}

/// https://en.wikipedia.org/wiki/Smoothstep
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let k = ((x - edge0) / (edge1 - edge0)).max(0.0).min(1.0);
    k * k * (3.0 - 2.0 * k)
}

/// Generates lightmap for given surface data with specified transform.
///
/// # Performance
///
/// This method is has linear complexity - the more complex mesh you pass, the more
/// time it will take. Required time increases drastically if you enable shadows (TODO) and
/// global illumination (TODO), because in this case your data will be raytraced.
pub fn generate_lightmap(
    data: &SurfaceSharedData,
    transform: &Mat4,
    lights: &[LightDefinition],
    texels_per_unit: u32,
) -> Texture {
    let vertices = transform_vertices(data, transform);
    let size = estimate_size(&vertices, &data.triangles, texels_per_unit);
    let mut pixels = Vec::<Pixel>::with_capacity((size * size) as usize);

    let scale = 1.0 / size as f32;

    let last_time = time::Instant::now();

    let grid = Grid::new(data, (size / 16).max(4) as usize);

    println!("Step 0: {:?}", time::Instant::now() - last_time);

    // TODO: Must be inverse transposed to eliminate scale/shear.
    let normal_matrix = transform.basis();

    let last_time = time::Instant::now();

    for y in 0..(size as usize) {
        for x in 0..(size as usize) {
            let uv = Vec2::new(x as f32 * scale, y as f32 * scale);

            if let Some((world_position, normal)) = pick(
                uv,
                &grid,
                &data.triangles,
                &data.vertices,
                &vertices,
                &normal_matrix,
            ) {
                pixels.push(Pixel::Color {
                    color: Color::BLACK,
                    position: world_position,
                    normal,
                })
            } else {
                pixels.push(Pixel::Transparent)
            }
        }
    }

    println!("Step 1: {:?}", time::Instant::now() - last_time);

    let last_time = time::Instant::now();

    for pixel in pixels.iter_mut() {
        if let Pixel::Color {
            color,
            position,
            normal,
        } = pixel
        {
            for light in lights {
                let (light_color, attenuation) = match light {
                    LightDefinition::Directional(directional) => {
                        let attenuation =
                            directional.intensity * lambertian(directional.direction, *normal);
                        (directional.color, attenuation)
                    }
                    LightDefinition::Spot(spot) => {
                        let d = *position - spot.position;
                        let distance = d.len();
                        let light_vec = d.scale(1.0 / distance);
                        let spot_angle_cos = light_vec.dot(&spot.direction);
                        let cone_factor = smoothstep(
                            ((spot.hotspot_cone_angle + spot.falloff_angle_delta) * 0.5).cos(),
                            (spot.hotspot_cone_angle * 0.5).cos(),
                            spot_angle_cos,
                        );
                        let attenuation = cone_factor
                            * spot.intensity
                            * lambertian(light_vec, *normal)
                            * distance_attenuation(distance, spot.distance);
                        (spot.color, attenuation)
                    }
                    LightDefinition::Point(point) => {
                        let d = *position - point.position;
                        let distance = d.len();
                        let light_vec = d.scale(1.0 / distance);
                        let attenuation = point.intensity
                            * lambertian(light_vec, *normal)
                            * distance_attenuation(distance, point.radius);
                        (point.color, attenuation)
                    }
                };
                color.r += ((light_color.r as f32) * attenuation) as u8;
                color.g += ((light_color.g as f32) * attenuation) as u8;
                color.b += ((light_color.b as f32) * attenuation) as u8;
            }
        }
    }

    println!("Step 2: {:?}", time::Instant::now() - last_time);

    let mut bytes = Vec::with_capacity((size * size * 4) as usize);
    for pixel in pixels {
        let color = match pixel {
            Pixel::Transparent => Color::TRANSPARENT,
            Pixel::Color { color, .. } => color,
        };
        bytes.push(color.r);
        bytes.push(color.g);
        bytes.push(color.b);
        bytes.push(color.a);
    }
    Texture::from_bytes(size, size, TextureKind::RGBA8, bytes).unwrap()
}

#[cfg(test)]
mod test {
    use crate::{
        core::{color::Color, math::vec3::Vec3},
        renderer::surface::SurfaceSharedData,
        utils::{
            lightmap::{generate_lightmap, LightDefinition, PointLightDefinition},
            uvgen::generate_uvs,
        },
    };
    use image::RgbaImage;

    #[test]
    fn test_generate_lightmap() {
        let mut data = SurfaceSharedData::make_sphere(100, 100, 1.0);

        generate_uvs(&mut data, 0.01);

        let lights = [LightDefinition::Point(PointLightDefinition {
            intensity: 3.0,
            position: Vec3::new(0.0, 2.0, 0.0),
            color: Color::WHITE,
            radius: 4.0,
        })];
        let lightmap = generate_lightmap(&data, &Default::default(), &lights, 128);

        let image = RgbaImage::from_raw(lightmap.width, lightmap.height, lightmap.bytes).unwrap();
        image.save("lightmap.png").unwrap();
    }
}