<div align="center">
  <img src="etc/320_orbit_logo.png" width="320" alt="Orbis logo">
<h1>Orbis - a modern Rust game engine</h1>
</div>



[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE-MIT.txt)
[![Rust Linux](https://github.com/AlessandroScrem/app/actions/workflows/rust-linux.yml/badge.svg)](https://github.com/AlessandroScrem/app/actions/workflows/rust-linux.yml)
[![Rust Windows](https://github.com/AlessandroScrem/app/actions/workflows/rust-windows.yml/badge.svg)](https://github.com/AlessandroScrem/app/actions/workflows/rust-windows.yml)
[![Rust macOS](https://github.com/AlessandroScrem/app/actions/workflows/rust-macos.yml/badge.svg)](https://github.com/AlessandroScrem/app/actions/workflows/rust-macos.yml)
[![Lines of Code](https://img.shields.io/endpoint?url=https%3A%2F%2Ftokei.kojix2.net%2Fapi%2Fbadge%2Flines%3Furl%3Dhttps%3A%2F%2Fgithub%2FAlessandroScrem%2Forbis.git)](https://github/AlessandroScrem/orbis)

# A modern 3D/2D render engine that uses wgpu 

```toml
[dependencies]
wgpu = "29"
imgui-wgpu = "0.28.0"
imgui = "0.12"
winit = "0.30.9"
legion = {version = "0.4.0", features = ["extended-tuple-impls"]}
cgmath = "0.18.0"
```


## Features

- [x] Ecs 
- [x] Phong shading
- [x] Imgui
- [x] Mesh entity
- [x] Light entity
- [x] SkyBox
- [x] IBL
- [x] PBR
- [x] Exposure
- [x] Tonemapping
- [x] Skybox selctor
- [x] xyz Axis
- [x] Docking Gui
- [x] Mesh Bounding box
- [x] Entity selection
- [x] Outline selection
- [x] Hierarchy entity
- [x] Import gltf with hierarchy
- [x] Material Editor per Submesh
- [x] Material gltf Transmission
- [x] Material gltf Volume
- [x] Material gltf Ior
- [x] Material gltf TextureTransform
- [x] Material gltf TextureCoords[0,1]
- [x] Material gltf Sheen
- [x] Environment rotation
- [x] Mesh Instance
- [x] Metarial preview
- [x] Object toggle visiblility 
- [x] Direct Light shadow
- [ ] SSAO   
- [ ] 

## Platforms
 - [x] Windows
 - [ ] Linux
 - [ ] MacOs

## Documentation

- [Resource IDs](docs/resource-id.md) — identity model, asset handles, GPU caches, runtime resources, and usage rules.

## Compiling and running

```bash
git clone https://github.com/AlessandroScrem/orbis.git
cd orbis

cargo build
cargo test --workspace --lib
cargo run -p orbis --release -- -v

// run with options 
cargo run -p orbis --release -- --help
cargo run -p orbis --release -- -w<WIDTH> -h<HEIGHT> --verbose
```


## Screenshots
![Hello PBR Cube](/crates/orbis-engine/assets/screenshots/hello_cube-2025-09-27.jpg?raw=true "Hello PBR cube!")

## Known issues
| Fixed    | Prioriry |              Description                                                   |
| :---:    | :---:    | :---                                                                       |
| `todo`   |   Low    | Background color is influenced by final filtering (gamma,  color filtering)|    
| `todo`   |   Low    | Environment / Skybox orientation different fromGltfViewer -90 Y            |    
| `fixed`  |   High   | Panic if click on bbox ui if no meshes in scene                            |    
| `fixed`  |   Low    | WGSL std140 / std430 uniform/storage buffer require allignemets to 16 bytes|    

