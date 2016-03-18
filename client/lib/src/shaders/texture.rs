//! Draw textures using a projection matrix.

use gl;
use yaglw::gl_context::GLContext;
use yaglw::shader::Shader;

/// Draw textures using a projection matrix.
pub struct TextureShader<'a> {
  #[allow(missing_docs)]
  pub shader: Shader<'a>,
}

impl<'a> TextureShader<'a> {
  #[allow(missing_docs)]
  pub fn new<'b>(gl: &'b GLContext) -> Self where 'a: 'b {
    let components = vec!(
      (gl::VERTEX_SHADER, "
        #version 330 core

        uniform mat4 projection_matrix;

        in vec3 position;
        in vec2 texture_position;

        out vec2 tex_position;

        void main() {
          tex_position = texture_position;
          gl_Position = projection_matrix * vec4(position, 1.0);
        }".to_owned()),
      (gl::FRAGMENT_SHADER, "
        #version 330 core

        uniform sampler2D texture_in;
        uniform float alpha_threshold;

        in vec2 tex_position;

        out vec4 frag_color;

        void main() {
          vec4 c = texture(texture_in, vec2(tex_position.x, 1.0 - tex_position.y));
          float x = 1;
          if (x == 0) {
            if (c.a < alpha_threshold) {
              discard;
            }
            frag_color = c;
            } else {
            frag_color = vec4(1, 0, 0, 1);
            }
        }".to_owned()),
    );
    TextureShader {
      shader: Shader::new(gl, components.into_iter()),
    }
  }
}
