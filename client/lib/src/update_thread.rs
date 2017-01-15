use cgmath;
use std::sync::Mutex;
use stopwatch;
use time;

use common::protocol;
use common::surroundings_loader;
use common::surroundings_loader::LoadType;

use audio_thread;
use chunk;
use client;
use lod;
use server_update::apply_server_update;
use terrain;
use view;

const MAX_OUTSTANDING_TERRAIN_REQUESTS: u32 = 1;

pub fn update_thread<RecvServer, UpdateView0, UpdateView1, UpdateAudio, UpdateServer, EnqueueTerrainLoad>(
  quit                 : &Mutex<bool>,
  client               : &client::T,
  recv_server          : &mut RecvServer,
  update_view0         : &mut UpdateView0,
  update_view1         : &mut UpdateView1,
  update_audio         : &mut UpdateAudio,
  update_server        : &mut UpdateServer,
  enqueue_terrain_load : &mut EnqueueTerrainLoad,
) where
  RecvServer         : FnMut() -> Option<protocol::ServerToClient>,
  UpdateView0        : FnMut(view::update::T),
  UpdateView1        : FnMut(view::update::T),
  UpdateAudio        : FnMut(audio_thread::Message),
  UpdateServer       : FnMut(protocol::ClientToServer),
  EnqueueTerrainLoad : FnMut(terrain::Load),
{
  'update_loop: loop {
    let should_quit = *quit.lock().unwrap();
    if should_quit {
      break 'update_loop
    } else {
      stopwatch::time("update_iteration", || {
        stopwatch::time("process_server_updates", || {
          process_server_updates(client, recv_server, update_view0, update_audio, update_server, enqueue_terrain_load);
        });

        stopwatch::time("update_surroundings", || {
          update_surroundings(client, update_view1, update_server);
        });

        stopwatch::time("process_voxel_updates", || {
          process_voxel_updates(client, update_view1);
        });
      })
    }
  }
}

#[inline(never)]
fn update_surroundings<UpdateView, UpdateServer>(
  client        : &client::T,
  update_view   : &mut UpdateView,
  update_server : &mut UpdateServer,
) where
  UpdateView   : FnMut(view::update::T),
  UpdateServer : FnMut(protocol::ClientToServer),
{
  let start = time::precise_time_ns();
  let mut i = 0;
  let player_position = *client.player_position.lock().unwrap();
  let player_position =
    cgmath::Point3::new(
      player_position.x.floor() as i32 >> chunk::LG_WIDTH,
      player_position.y.floor() as i32 >> chunk::LG_WIDTH,
      player_position.z.floor() as i32 >> chunk::LG_WIDTH,
    );
  let mut surroundings_loader = client.surroundings_loader.lock().unwrap();
  let mut updates = surroundings_loader.updates(&player_position) ;
  let mut terrain = client.terrain.lock().unwrap();
  loop {
    if client.pending_terrain_requests.lock().unwrap().len() as u32 >= MAX_OUTSTANDING_TERRAIN_REQUESTS {
      trace!("update loop breaking");
      break;
    }

    let chunk_position;
    let load_type;
    match updates.next() {
      None => break,
      Some((p, l)) => {
        chunk_position = p;
        load_type = l;
      },
    }

    debug!("chunk surroundings");
    let distance =
      surroundings_loader::distance_between(
        &player_position,
        &chunk_position,
      );
    let new_lod = lod::of_distance(distance);
    let lg_voxel_size = new_lod.lg_sample_size();
    let chunk_position = chunk::position::T { as_point: chunk_position };
    match load_type {
      LoadType::Load | LoadType::Downgrade => {
        let r =
          terrain.try_load_chunk(
            &client.id_allocator,
            &mut *client.rng.lock().unwrap(),
            update_view,
            &chunk::position::T { as_point: player_position },
            &chunk_position,
            new_lod,
          );
        use terrain::LoadResult::*;
        match r {
          Success | AlreadyLoaded => {},
          ChunkMissing => {
            let request_already_exists =
              !client.pending_terrain_requests
                .lock().unwrap()
                .insert((chunk_position, lg_voxel_size));
            if !request_already_exists {
              update_server(
                protocol::ClientToServer::RequestChunk {
                  time_requested_ns : time::precise_time_ns(),
                  client_id       : client.id,
                  position        : chunk_position,
                  lg_voxel_size   : lg_voxel_size,
                }
              );
            }
          },
        }
      },
      LoadType::Unload => {
        terrain.unload(update_view, &chunk_position);
      },
    }

    if i >= 10 {
      i -= 10;
      if time::precise_time_ns() - start >= 1_000_000 {
        break
      }
    }
    i += 1;
  }
}

fn process_voxel_updates<UpdateView>(
  client      : &client::T,
  update_view : &mut UpdateView,
) where
  UpdateView: FnMut(view::update::T),
{
  let terrain = &mut *client.terrain.lock().unwrap();
  let rng = &mut *client.rng.lock().unwrap();
  let player_position = chunk::position::of_world_position(&*client.player_position.lock().unwrap());
  terrain.tick(&client.id_allocator, rng, update_view, &player_position);
}

#[inline(never)]
fn process_server_updates<RecvServer, UpdateView, UpdateAudio, UpdateServer, EnqueueTerrainLoad>(
  client               : &client::T,
  recv_server          : &mut RecvServer,
  update_view          : &mut UpdateView,
  update_audio         : &mut UpdateAudio,
  update_server        : &mut UpdateServer,
  enqueue_terrain_load : &mut EnqueueTerrainLoad,
) where
  RecvServer         : FnMut() -> Option<protocol::ServerToClient>,
  UpdateView         : FnMut(view::update::T),
  UpdateAudio        : FnMut(audio_thread::Message),
  UpdateServer       : FnMut(protocol::ClientToServer),
  EnqueueTerrainLoad : FnMut(terrain::Load),
{
  let start = time::precise_time_ns();
  let mut i = 0;
  while let Some(up) = recv_server() {
    apply_server_update(
      client,
      update_view,
      update_audio,
      update_server,
      enqueue_terrain_load,
      up,
    );

    if i > 10 {
      i -= 10;
      if time::precise_time_ns() - start >= 1_000_000 {
        break
      }
    }
    i += 1;
  }
}
