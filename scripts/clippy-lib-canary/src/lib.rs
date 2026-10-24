//! Each function uses one form that `.github/clippy-lib/clippy.toml` forbids
//! `scripts/clippy_lib_gate.py` runs clippy with that configuration and
//! requires a `disallowed_methods` diagnostic for every one of them

pub fn method_f32(a: f32, b: f32) -> f32 {
    a.max(b)
}

pub fn path_f32(a: f32, b: f32) -> f32 {
    f32::min(a, b)
}

pub fn method_f64(a: f64, b: f64) -> f64 {
    a.min(b)
}

pub fn fold_f64(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}

// representative libm forms, to prove the generated lib config still carries
// the full crate-root libm list (alice-lol/clippy.toml), not just the max/min
// entries this script adds on top of it

pub fn libm_sin_cos(x: f32) -> (f32, f32) {
    x.sin_cos()
}

pub fn libm_atan2(y: f32, x: f32) -> f32 {
    y.atan2(x)
}

pub fn libm_tan(z: f32) -> f32 {
    z.tan()
}

// glam vectors: lane-wise platform min / max, one function per form

use glam::{Vec2, Vec3, Vec3A, Vec4};

pub fn glam_min(p: Vec3, q: Vec3) -> Vec3 {
    p.min(q)
}

pub fn glam_max(p: Vec3, q: Vec3) -> Vec3 {
    p.max(q)
}

pub fn glam_min_element(p: Vec3) -> f32 {
    p.min_element()
}

pub fn glam_max_element(p: Vec3) -> f32 {
    p.max_element()
}

pub fn glam_clamp(p: Vec3, q: Vec3, r: Vec3) -> Vec3 {
    p.clamp(q, r)
}

pub fn glam_path_min(p: Vec3, q: Vec3) -> Vec3 {
    Vec3::min(p, q)
}

pub fn glam_vec2_max(u: Vec2, w: Vec2) -> Vec2 {
    u.max(w)
}

pub fn glam_vec4_min_element(h: Vec4) -> f32 {
    h.min_element()
}

pub fn glam_vec3a_clamp(k: Vec3A) -> Vec3A {
    k.clamp(k, k)
}

// the rest of the glam entries, so every library-only entry has a form (a mistyped or removed
// entry forbids nothing and clippy does not say so for a method path)

pub fn glam_vec2_min(a2: Vec2, a2_o: Vec2) -> Vec2 {
    a2.min(a2_o)
}

pub fn glam_vec2_min_element(a2: Vec2) -> f32 {
    a2.min_element()
}

pub fn glam_vec2_max_element(a2: Vec2) -> f32 {
    a2.max_element()
}

pub fn glam_vec2_clamp(a2: Vec2, a2_lo: Vec2, a2_hi: Vec2) -> Vec2 {
    a2.clamp(a2_lo, a2_hi)
}

pub fn glam_vec3a_min(a3: Vec3A, a3_o: Vec3A) -> Vec3A {
    a3.min(a3_o)
}

pub fn glam_vec3a_max(a3: Vec3A, a3_o: Vec3A) -> Vec3A {
    a3.max(a3_o)
}

pub fn glam_vec3a_min_element(a3: Vec3A) -> f32 {
    a3.min_element()
}

pub fn glam_vec3a_max_element(a3: Vec3A) -> f32 {
    a3.max_element()
}

pub fn glam_vec4_min(a4: Vec4, a4_o: Vec4) -> Vec4 {
    a4.min(a4_o)
}

pub fn glam_vec4_max(a4: Vec4, a4_o: Vec4) -> Vec4 {
    a4.max(a4_o)
}

pub fn glam_vec4_max_element(a4: Vec4) -> f32 {
    a4.max_element()
}

pub fn glam_vec4_clamp(a4: Vec4, a4_lo: Vec4, a4_hi: Vec4) -> Vec4 {
    a4.clamp(a4_lo, a4_hi)
}

// ALICE-Law G10: the wall clock and OS-seeded hashing (each line one form of scripts/clippy_lib_gate.toml)
pub fn clock_instant_now() -> std::time::Instant {
    std::time::Instant::now()
}

pub fn clock_system_now() -> std::time::SystemTime {
    std::time::SystemTime::now()
}

pub fn clock_instant_elapsed(started: std::time::Instant) -> std::time::Duration {
    started.elapsed()
}

pub fn clock_system_elapsed(stamp: std::time::SystemTime) -> Result<std::time::Duration, std::time::SystemTimeError> {
    stamp.elapsed()
}

pub fn seeded_state() {
    let _ = std::collections::hash_map::RandomState::new();
}

pub fn seeded_param(_seeded: std::collections::hash_map::RandomState) {}

pub fn held_clock(later: std::time::Instant, earlier: std::time::Instant) -> std::time::Duration {
    later.duration_since(earlier)
}

// ALICE-Law G10: the iteration order of a HashMap / HashSet (one form per method of scripts/clippy_lib_gate.toml)
type Hm = std::collections::HashMap<u8, u8>;
type Hs = std::collections::HashSet<u8>;

pub fn hash_iter(hm: &Hm) -> usize {
    hm.iter().count()
}

pub fn hash_keys(hm: &Hm) -> usize {
    hm.keys().count()
}

pub fn hash_values(hm: &Hm) -> usize {
    hm.values().count()
}

pub fn hash_iter_mut(hm_mut: &mut Hm) -> usize {
    hm_mut.iter_mut().count()
}

pub fn hash_values_mut(hm_mut: &mut Hm) -> usize {
    hm_mut.values_mut().count()
}

pub fn hash_drain(hm_mut: &mut Hm) -> usize {
    hm_mut.drain().count()
}

pub fn hash_into_keys(hm_k: Hm) -> usize {
    hm_k.into_keys().count()
}

pub fn hash_into_values(hm_v: Hm) -> usize {
    hm_v.into_values().count()
}

pub fn set_iter(hs: &Hs) -> usize {
    hs.iter().count()
}

pub fn set_drain(hs_mut: &mut Hs) -> usize {
    hs_mut.drain().count()
}

// ALICE-Law G10: a for loop over a hash type (scripts/hash_iter_canary.py: clippy::iter_over_hash_type)
pub fn hash_for(hm: &Hm) -> u32 {
    let mut n = 0;
    for _ in hm { // for-over-hash
        n += 1;
    }
    n
}

pub fn set_for(hs: Hs) -> u32 {
    let mut n = 0;
    for _ in hs { // for-over-hash
        n += 1;
    }
    n
}

// ALICE-Law G10: more hash-order forms, OS-assigned values and the environment (scripts/clippy_lib_gate.toml)
pub fn set_union(hs: &Hs, hs_o: &Hs) -> usize {
    hs.union(&hs_o).count()
}

pub fn set_difference(hs: &Hs, hs_o: &Hs) -> usize {
    hs.difference(&hs_o).count()
}

pub fn set_intersection(hs: &Hs, hs_o: &Hs) -> usize {
    hs.intersection(&hs_o).count()
}

pub fn set_symmetric_difference(hs: &Hs, hs_o: &Hs) -> usize {
    hs.symmetric_difference(&hs_o).count()
}

pub fn set_retain(hs_mut: &mut Hs) {
    hs_mut.retain(|_| true);
}

pub fn set_extract_if(hs_mut: &mut Hs) -> usize {
    hs_mut.extract_if(|_| false).count()
}

pub fn map_retain(hm_mut: &mut Hm) {
    hm_mut.retain(|_, _| true);
}

pub fn map_extract_if(hm_mut: &mut Hm) -> usize {
    hm_mut.extract_if(|_, _| false).count()
}

pub fn os_process_id() -> u32 {
    std::process::id()
}

pub fn os_thread_id() -> std::thread::ThreadId {
    std::thread::current().id()
}

pub fn env_var() -> bool {
    std::env::var("X").is_ok()
}

pub fn env_var_os() -> bool {
    std::env::var_os("X").is_some()
}

pub fn env_vars() -> usize {
    std::env::vars().count()
}

pub fn env_vars_os() -> usize {
    std::env::vars_os().count()
}

// --- alice-sdf writers (generated by scripts/sdf_writers.py --write) ---
// one call of each alice-sdf writer the library may not call; the arguments only type-check
#[allow(unreachable_code, unused_must_use, clippy::diverging_sub_expression)]
pub fn sdf_writers() {
    let _ = alice_sdf::io::save_abm(todo!(), "x");
    let _ = alice_sdf::io::save_abm_with_lods(todo!(), todo!(), "x");
    let _ = alice_sdf::io::save_abm_with_lods_with_provenance(todo!(), todo!(), "x", todo!());
    let _ = alice_sdf::io::save_abm_with_provenance(todo!(), "x", todo!());
    let _ = alice_sdf::io::AbmHeader::to_bytes(todo!());
    let _ = alice_sdf::io::export_alembic(todo!(), "x", todo!());
    let _ = alice_sdf::io::export_alembic_with_provenance(todo!(), "x", todo!(), todo!());
    let _ = alice_sdf::io::save_asdf(todo!(), "x");
    let _ = alice_sdf::io::save_asdf_with_provenance(todo!(), "x", todo!());
    let _ = alice_sdf::io::AsdfHeader::to_bytes(todo!());
    let _ = alice_sdf::io::export_fbx(todo!(), "x", todo!(), todo!());
    let _ = alice_sdf::io::export_fbx_with_provenance(todo!(), "x", todo!(), todo!(), todo!());
    let _ = alice_sdf::io::export_glb(todo!(), "x", todo!(), todo!());
    let _ = alice_sdf::io::export_glb_bytes(todo!(), todo!(), todo!());
    let _ = alice_sdf::io::export_glb_bytes_with_provenance(todo!(), todo!(), todo!(), todo!());
    let _ = alice_sdf::io::export_glb_with_provenance(todo!(), "x", todo!(), todo!(), todo!());
    let _ = alice_sdf::io::export_gltf_json(todo!(), "x", todo!(), todo!());
    let _ = alice_sdf::io::export_gltf_json_with_provenance(todo!(), "x", todo!(), todo!(), todo!());
    let _ = alice_sdf::io::iges::export_iges("x", todo!(), todo!());
    let _ = alice_sdf::io::iges::export_iges_with_provenance("x", todo!(), todo!(), todo!());
    let _ = alice_sdf::io::save_asdf_json(todo!(), "x");
    let _ = alice_sdf::io::save_asdf_json_with_provenance(todo!(), "x", todo!());
    let _ = alice_sdf::io::to_json_string(todo!());
    let _ = alice_sdf::io::to_json_string_with_provenance(todo!(), todo!());
    let _ = alice_sdf::io::meshopt_gltf::export_glb_meshopt(todo!(), "x", todo!());
    let _ = alice_sdf::io::meshopt_gltf::export_glb_meshopt_bytes(todo!(), todo!());
    let _ = alice_sdf::io::meshopt_gltf::export_glb_meshopt_bytes_skinned(todo!(), todo!(), todo!());
    let _ = alice_sdf::io::meshopt_gltf::export_glb_meshopt_bytes_skinned_with_provenance(todo!(), todo!(), todo!(), todo!());
    let _ = alice_sdf::io::meshopt_gltf::export_glb_meshopt_skinned(todo!(), todo!(), "x", todo!());
    let _ = alice_sdf::io::export_nanite(todo!(), "x");
    let _ = alice_sdf::io::nanite::export_nanite_hlsl_material(todo!(), "x");
    let _ = alice_sdf::io::nanite::export_nanite_hlsl_material_with_provenance(todo!(), "x", todo!());
    let _ = alice_sdf::io::export_nanite_json(todo!(), "x");
    let _ = alice_sdf::io::export_nanite_json_with_provenance(todo!(), "x", todo!());
    let _ = alice_sdf::io::export_nanite_with_config(todo!(), "x", todo!());
    let _ = alice_sdf::io::export_nanite_with_provenance(todo!(), "x", todo!(), todo!());
    let _ = alice_sdf::io::export_obj(todo!(), "x", todo!(), todo!());
    let _ = alice_sdf::io::export_obj_with_provenance(todo!(), "x", todo!(), todo!(), todo!());
    let _ = alice_sdf::io::export_ply(todo!(), "x", todo!());
    let _ = alice_sdf::io::export_ply_with_provenance(todo!(), "x", todo!(), todo!());
    let _ = alice_sdf::io::splat::save_splat("x", todo!());
    let _ = alice_sdf::io::splat::save_splat_with_provenance("x", todo!(), todo!());
    let _ = alice_sdf::io::splat::Splat::to_bytes(todo!());
    let _ = alice_sdf::io::step::export_step("x", todo!(), todo!());
    let _ = alice_sdf::io::step::export_step_with_provenance("x", todo!(), todo!(), todo!());
    let _ = alice_sdf::io::export_stl(todo!(), "x");
    let _ = alice_sdf::io::export_stl_ascii(todo!(), "x");
    let _ = alice_sdf::io::export_stl_ascii_with_provenance(todo!(), "x", todo!());
    let _ = alice_sdf::io::export_stl_with_provenance(todo!(), "x", todo!());
    let _ = alice_sdf::io::export_3mf(todo!(), "x");
    let _ = alice_sdf::io::export_3mf_with_provenance(todo!(), "x", todo!());
    let _ = alice_sdf::io::export_ue5_mesh(todo!(), "x", todo!());
    let _ = alice_sdf::io::export_ue5_mesh_binary(todo!(), "x", todo!());
    let _ = alice_sdf::io::export_ue5_mesh_binary_with_provenance(todo!(), "x", todo!(), todo!());
    let _ = alice_sdf::io::export_ue5_mesh_with_lods(todo!(), todo!(), "x", todo!());
    let _ = alice_sdf::io::export_ue5_mesh_with_lods_with_provenance(todo!(), todo!(), "x", todo!(), todo!());
    let _ = alice_sdf::io::export_ue5_mesh_with_provenance(todo!(), "x", todo!(), todo!());
    let _ = alice_sdf::io::export_unity_mesh(todo!(), "x", todo!());
    let _ = alice_sdf::io::export_unity_mesh_binary(todo!(), "x", todo!());
    let _ = alice_sdf::io::export_unity_mesh_binary_with_provenance(todo!(), "x", todo!(), todo!());
    let _ = alice_sdf::io::export_unity_mesh_with_provenance(todo!(), "x", todo!(), todo!());
    let _ = alice_sdf::io::export_usda(todo!(), "x", todo!(), todo!());
    let _ = alice_sdf::io::export_usda_with_provenance(todo!(), "x", todo!(), todo!(), todo!());
    let _ = alice_sdf::io::vox::save_vox("x", todo!());
    let _ = alice_sdf::io::vox::save_vox_with_provenance("x", todo!(), todo!());
    let _ = alice_sdf::mesh::save_lod_chain(todo!(), "x");
    let _ = alice_sdf::mesh::save_lod_chain_metadata(todo!(), "x");
    let _ = alice_sdf::mesh::save_lod_chain_metadata_with_provenance(todo!(), "x", todo!());
    let _ = alice_sdf::mesh::save_lod_chain_with_provenance(todo!(), "x", todo!());
    let _ = alice_sdf::mesh::mesh_codec::encode_indices(todo!());
    let _ = alice_sdf::mesh::mesh_codec::encode_indices_with_provenance(todo!(), todo!());
    let _ = alice_sdf::mesh::mesh_codec::encode_mesh(todo!());
    let _ = alice_sdf::mesh::mesh_codec::encode_mesh_with_provenance(todo!(), todo!());
    let _ = alice_sdf::mesh::mesh_codec::encode_positions(todo!());
    let _ = alice_sdf::mesh::mesh_codec::encode_positions_with_provenance(todo!(), todo!());
    let _ = alice_sdf::mesh::meshopt_index_codec::encode_index_buffer(todo!());
    let _ = alice_sdf::mesh::meshopt_index_codec::try_encode_index_buffer(todo!());
    let _ = alice_sdf::mesh::meshopt_index_codec::try_encode_index_buffer_with_provenance(todo!(), todo!());
    let _ = alice_sdf::mesh::meshopt_vertex_codec::encode_vertex_buffer(todo!(), todo!());
    let _ = alice_sdf::mesh::meshopt_vertex_codec::encode_vertex_buffer_level(todo!(), todo!(), todo!());
    let _ = alice_sdf::mesh::meshopt_vertex_codec::encode_vertex_buffer_with_provenance(todo!(), todo!(), todo!());
    let _ = alice_sdf::neural::NeuralSdf::save(todo!(), &mut std::io::sink());
    let _ = alice_sdf::neural::NeuralSdf::save_with_provenance(todo!(), &mut std::io::sink(), todo!());
    let _ = alice_sdf::validity::export_step_validated("x", todo!(), todo!(), todo!());
    let _ = alice_sdf::validity::export_step_validated_with_provenance("x", todo!(), todo!(), todo!(), todo!());
}
// --- end alice-sdf writers ---
