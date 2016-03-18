//! Draw the view.

use camera::set_camera;
use cgmath;
use gl;
use std;
use view;

fn draw_backdrop(
  rndr: &mut view::T,
) {
  rndr.shaders.clouds.shader.use_shader(&mut rndr.gl);

  unsafe {
    gl::BindVertexArray(rndr.empty_gl_array.gl_id);
  }

  unsafe {
    let sun_uniform = rndr.shaders.clouds.shader.get_uniform_location("sun_color");
    let ptr = std::mem::transmute(&rndr.sun.intensity);
    gl::Uniform3fv(sun_uniform, 1, ptr);

    let eye_uniform = rndr.shaders.clouds.shader.get_uniform_location("eye_position");
    let ptr = std::mem::transmute(&rndr.camera.position);
    gl::Uniform3fv(eye_uniform, 1, ptr);

    let projection_uniform = rndr.shaders.clouds.shader.get_uniform_location("projection_matrix");
    let projection_matrix = rndr.camera.projection_matrix();
    let ptr = std::mem::transmute(&projection_matrix);
    gl::UniformMatrix4fv(projection_uniform, 1, 0, ptr);

    let window_size_uniform = rndr.shaders.clouds.shader.get_uniform_location("window_size");
    let window_size = cgmath::Vector2::new(rndr.window_size.x as f32, rndr.window_size.y as f32);
    let ptr = std::mem::transmute(&window_size);
    gl::Uniform2fv(window_size_uniform, 1, ptr);

    gl::DrawArrays(gl::TRIANGLE_STRIP, 0, 4);

    gl::Clear(gl::DEPTH_BUFFER_BIT);
  }
}

#[allow(missing_docs)]
pub fn render(
  rndr: &mut view::T,
) {
  rndr.gl.clear_buffer();

  draw_backdrop(rndr);

  unsafe {
    gl::Enable(gl::CULL_FACE);
  }

  // draw the world
  rndr.shaders.terrain_shader.shader.use_shader(&mut rndr.gl);
  set_camera(&mut rndr.shaders.terrain_shader.shader, &mut rndr.gl, &rndr.camera);
  rndr.terrain_buffers.draw(&mut rndr.gl);

  rndr.shaders.mob_shader.shader.use_shader(&mut rndr.gl);
  set_camera(&mut rndr.shaders.mob_shader.shader, &mut rndr.gl, &rndr.camera);
  rndr.mob_buffers.draw(&mut rndr.gl);
  rndr.player_buffers.draw(&mut rndr.gl);

  rndr.shaders.texture_shader.shader.use_shader(&mut rndr.gl);
  set_camera(&mut rndr.shaders.texture_shader.shader, &mut rndr.gl, &rndr.camera);
  let alpha_threshold_uniform =
    rndr.shaders.texture_shader.shader.get_uniform_location("alpha_threshold");
  unsafe {
    gl::Disable(gl::CULL_FACE);
    gl::Uniform1f(alpha_threshold_uniform, 0.5);
    gl::ActiveTexture(rndr.misc_texture_unit.gl_id());
    gl::BindTexture(gl::TEXTURE_2D, rndr.grass_texture.handle.gl_id);
  }
  rndr.grass_buffers.draw(&mut rndr.gl);

  if rndr.show_hud {
    rndr.shaders.hud_color_shader.shader.use_shader(&mut rndr.gl);
    rndr.hud_triangles.bind(&mut rndr.gl);
    rndr.hud_triangles.draw(&mut rndr.gl);
  }
}
