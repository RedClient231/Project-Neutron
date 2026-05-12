//! GPU Passthrough for Mali (Helio G99 / Mali-G57 MC2).
//!
//! Games need direct GPU access. On Android without root, the GPU driver
//! is accessed via /dev/mali0 and the OpenGL ES / Vulkan userspace drivers.
//! We ensure the virtual process can access these without interference.

use neutron_core::{NeutronError, NeutronResult};
use log::{debug, info};

/// GPU passthrough configuration for Mali GPUs.
pub struct GpuPassthrough {
    /// Whether GPU passthrough is enabled
    enabled: bool,
    /// GPU device node path
    device_path: String,
    /// Detected GPU model
    gpu_model: String,
    /// OpenGL ES version supported
    gles_version: (u32, u32),
}

impl GpuPassthrough {
    /// Create GPU passthrough for Mali-G57 (Helio G99).
    pub fn new() -> Self {
        Self {
            enabled: true,
            device_path: "/dev/mali0".into(),
            gpu_model: "Mali-G57 MC2".into(),
            gles_version: (3, 2),
        }
    }

    /// Check if the GPU device is accessible.
    pub fn check_gpu_access(&self) -> NeutronResult<bool> {
        let accessible = std::path::Path::new(&self.device_path).exists();
        if accessible {
            info!("GPU device accessible: {}", self.device_path);
        } else {
            debug!("GPU device not directly accessible (normal for non-root)");
        }
        Ok(accessible)
    }

    /// Get paths that must NOT be redirected (GPU driver paths).
    /// These must pass through to the real filesystem.
    pub fn passthrough_paths(&self) -> Vec<&str> {
        vec![
            "/dev/mali0",
            "/dev/mali",
            "/dev/dri/",
            "/vendor/lib64/egl/",
            "/vendor/lib/egl/",
            "/vendor/lib64/hw/gralloc.",
            "/vendor/lib/hw/gralloc.",
            "/system/lib64/libEGL.so",
            "/system/lib/libEGL.so",
            "/system/lib64/libGLESv2.so",
            "/system/lib/libGLESv2.so",
            "/system/lib64/libGLESv3.so",
            "/system/lib64/libvulkan.so",
            "/system/lib/libvulkan.so",
            "/vendor/lib64/libvulkan.so",
            "/vendor/lib/libvulkan.so",
            "/vendor/lib64/vulkan.",
            "/vendor/lib/vulkan.",
            "/system/lib64/libOpenCL.so",
            "/vendor/lib64/libOpenCL.so",
        ]
    }

    /// Get environment variables needed for GPU access.
    pub fn env_vars(&self) -> Vec<(String, String)> {
        vec![
            // Don't use software renderer
            ("LIBGL_ALWAYS_SOFTWARE".into(), "0".into()),
            // Mali specific
            ("MALI_VISIBLE_DEVICE".into(), "0".into()),
            // Use system EGL/GLES
            ("EGL_PLATFORM".into(), "android".into()),
        ]
    }

    /// Check if a path is a GPU-related path that should pass through.
    pub fn is_gpu_path(&self, path: &str) -> bool {
        self.passthrough_paths().iter().any(|p| path.starts_with(p))
    }

    /// Get the GPU model string for property spoofing.
    pub fn model_string(&self) -> &str {
        &self.gpu_model
    }

    /// Is GPU passthrough enabled?
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for GpuPassthrough {
    fn default() -> Self {
        Self::new()
    }
}
