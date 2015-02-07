use id_allocator::IdAllocator;
use in_progress_terrain::InProgressTerrain;
use lod_map::{LOD, OwnerId, LODMap};
use noise::Seed;
use opencl_context::CL;
use physics::Physics;
use shaders::terrain::TerrainShader;
use state::EntityId;
use std::iter::repeat;
use stopwatch::TimerSet;
use terrain::terrain::Terrain;
use terrain::terrain_block::{BlockPosition, BLOCK_WIDTH};
use terrain::texture_generator::TEXTURE_WIDTH;
use terrain::texture_generator::TerrainTextureGenerator;
use terrain::terrain_vram_buffers::TerrainVRAMBuffers;
use yaglw::gl_context::GLContext;
use yaglw::texture::TextureUnit;

/// Load and unload TerrainBlocks from the game.
/// Each TerrainBlock can be owned by a set of owners, each of which can independently request LODs.
/// The maximum LOD requested is the one that is actually loaded.
pub struct TerrainGameLoader<'a> {
  terrain: Terrain,
  texture_generators: [TerrainTextureGenerator; 4],
  vram_buffers: TerrainVRAMBuffers<'a>,
  in_progress_terrain: InProgressTerrain,
  // The LODs of the currently loaded blocks.
  lod_map: LODMap,
}

impl<'a> TerrainGameLoader<'a> {
  pub fn new(
    gl: &'a mut GLContext,
    cl: &CL,
    shader: &mut TerrainShader,
    texture_unit_alloc: &mut IdAllocator<TextureUnit>,
  ) -> TerrainGameLoader<'a> {
    let vram_buffers = TerrainVRAMBuffers::new(gl);
    vram_buffers.bind_glsl_uniforms(gl, texture_unit_alloc, shader);

    TerrainGameLoader {
      terrain: Terrain::new(Seed::new(0), 0),
      texture_generators: [
        TerrainTextureGenerator::new(cl, TEXTURE_WIDTH[0], BLOCK_WIDTH as u32),
        TerrainTextureGenerator::new(cl, TEXTURE_WIDTH[1], BLOCK_WIDTH as u32),
        TerrainTextureGenerator::new(cl, TEXTURE_WIDTH[2], BLOCK_WIDTH as u32),
        TerrainTextureGenerator::new(cl, TEXTURE_WIDTH[3], BLOCK_WIDTH as u32),
      ],
      vram_buffers: vram_buffers,
      in_progress_terrain: InProgressTerrain::new(),
      lod_map: LODMap::new(),
    }
  }

  /// Returns false if pushing into buffers fails.
  fn re_lod_block(
    &mut self,
    timers: &TimerSet,
    gl: &mut GLContext,
    cl: &CL,
    id_allocator: &mut IdAllocator<EntityId>,
    physics: &mut Physics,
    block_position: &BlockPosition,
    loaded_lod: Option<LOD>,
    new_lod: Option<LOD>,
  ) -> bool {
    // Unload whatever's there.
    match loaded_lod {
      None => {},
      Some(LOD::Placeholder) => {
        self.in_progress_terrain.remove(physics, block_position);
      }
      Some(LOD::LodIndex(loaded_lod)) => {
        timers.time("terrain_game_loader.unload", || {
          let lods =
            self.terrain.all_blocks.get(block_position)
            .unwrap()
            .lods
            .as_slice();
          let block = lods[loaded_lod as usize].as_ref().unwrap();
          for id in block.ids.iter() {
            physics.remove_terrain(*id);
            self.vram_buffers.swap_remove(gl, *id);
          }

          self.vram_buffers.free_block_data(loaded_lod, block_position);
        });
      },
    }

    // TODO: Avoid the double-lookup when loaded_lod and new_lod are both LodIndexes.

    // Load whatever we should be loading.
    match new_lod {
      None => true,
      Some(LOD::Placeholder) => {
        self.in_progress_terrain.insert(id_allocator, physics, block_position);
        true
      },
      Some(LOD::LodIndex(new_lod)) => {
        timers.time("terrain_game_loader.load", || {
          let vram_buffers = &mut self.vram_buffers;
          self.terrain.load(
            timers,
            cl,
            &self.texture_generators[new_lod as usize],
            id_allocator,
            block_position,
            new_lod,
            |block| {
              timers.time("terrain_game_loader.load.physics", || {
                for &(ref id, ref bounds) in block.bounds.iter() {
                  physics.insert_terrain(*id, bounds.clone());
                }
              });

              timers.time("terrain_game_loader.load.gpu", || {
                if block.ids.is_empty() {
                  true
                } else {
                  let block_index =
                    vram_buffers.push_block_data(
                      gl,
                      *block_position,
                      block.pixels.as_slice(),
                      new_lod,
                    );

                  let block_indices: Vec<_> =
                    repeat(block_index).take(block.ids.len()).collect();

                  let success =
                    vram_buffers.push(
                      gl,
                      block.vertex_coordinates.as_slice(),
                      block.normals.as_slice(),
                      block.coords.as_slice(),
                      block_indices.as_slice(),
                      block.ids.as_slice(),
                    );

                  success
                }
              })
            },
          )
        })
      },
    }
  }

  pub fn increase_lod(
    &mut self,
    timers: &TimerSet,
    gl: &mut GLContext,
    cl: &CL,
    id_allocator: &mut IdAllocator<EntityId>,
    physics: &mut Physics,
    block_position: &BlockPosition,
    target_lod: LOD,
    owner: OwnerId,
  ) -> bool {
    let (prev_lod, lod_change) =
      self.lod_map.increase_lod(*block_position, target_lod, owner);

    match lod_change {
      None => true,
      Some(lod_change) => {
        let success =
          self.re_lod_block(
            timers,
            gl,
            cl,
            id_allocator,
            physics,
            block_position,
            lod_change.loaded,
            lod_change.desired,
          );
        if !success {
          // We failed to change LOD. Revert the lod_map entry.

          self.lod_map.decrease_lod(*block_position, prev_lod, owner);
        }
        success
      },
    }
  }

  pub fn decrease_lod(
    &mut self,
    timers: &TimerSet,
    gl: &mut GLContext,
    cl: &CL,
    id_allocator: &mut IdAllocator<EntityId>,
    physics: &mut Physics,
    block_position: &BlockPosition,
    target_lod: Option<LOD>,
    owner: OwnerId,
  ) -> bool {
    let (prev_lod, lod_change) =
      self.lod_map.decrease_lod(*block_position, target_lod, owner);

    match lod_change {
      None => true,
      Some(lod_change) => {
        let success =
          self.re_lod_block(
            timers,
            gl,
            cl,
            id_allocator,
            physics,
            block_position,
            lod_change.loaded,
            lod_change.desired,
          );
        if !success {
          // We failed to change LOD. Revert the lod_map entry.

          prev_lod
            .map(|prev_lod| self.lod_map.increase_lod(*block_position, prev_lod, owner));
        }
        success
      },
    }
  }

  pub fn draw(&self, gl: &mut GLContext) {
    self.vram_buffers.draw(gl);
  }
}
