//! What the renderer is asked for. Matters on Windows and nowhere else.
//!
//! On DX12 wgpu hands Bevy's shaders (HLSL made from WGSL) to a compiler,
//! and Bevy's default is FXC unless `dxcompiler.dll` lies in the *working
//! directory*. FXC is old and slow: every pipeline of the PBR material our
//! tiles, props and tokens are drawn with takes it seconds to minutes, and
//! the artist waited minutes at every start. DXC does the same in well
//! under a second. `scripts/build-windows.sh` puts Microsoft's
//! `dxcompiler.dll` and `dxil.dll` beside the exe, and we point wgpu at
//! them by the exe's own path, so a shortcut that starts the game from
//! another folder still finds them.
//!
//! Bevy's `statically-linked-dxc` would need no DLLs, but it pulls
//! `mach-dxcompiler-rs`, whose build script refuses (since 0.1.6) to
//! download its prebuilt DXC from a mutable GitHub release. So the DLLs
//! come from Microsoft's release, pinned by hash (`scripts/fetch-dxc.sh`).
//!
//! Compiled pipelines cannot ship in the build: wgpu's cache blobs are
//! keyed to the adapter and driver, and Bevy does not load them anyway.
//!
//! Everything here is a default: `WGPU_BACKEND=dx12|vulkan` and
//! `WGPU_DX12_COMPILER=fxc|dxc` still win, so a machine that misbehaves can
//! try the other path without a new build.

use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};

pub fn render_plugin() -> RenderPlugin {
    #[allow(unused_mut)]
    let mut settings = WgpuSettings::default();

    #[cfg(target_os = "windows")]
    {
        use bevy::render::settings::{Backends, Dx12Compiler};

        // No GL: a fallback that draws slower and differently is worse than
        // a clear failure, and every machine this ships to has DX12.
        settings.backends = Backends::from_env().or(Some(Backends::VULKAN | Backends::DX12));

        let beside_exe = std::env::current_exe()
            .ok()
            .and_then(|exe| Some(exe.parent()?.join("dxcompiler.dll")))
            .filter(|dll| dll.is_file());
        settings.dx12_shader_compiler = Dx12Compiler::from_env().unwrap_or(match beside_exe {
            Some(dll) => Dx12Compiler::DynamicDxc {
                dxc_path: dll.to_string_lossy().into_owned(),
            },
            None => {
                bevy::log::warn!("no dxcompiler.dll beside the exe: DX12 falls back to FXC");
                Dx12Compiler::Auto
            }
        });
        bevy::log::info!("dx12 shader compiler: {:?}", settings.dx12_shader_compiler);
    }

    RenderPlugin {
        render_creation: RenderCreation::Automatic(Box::new(settings)),
        ..Default::default()
    }
}
