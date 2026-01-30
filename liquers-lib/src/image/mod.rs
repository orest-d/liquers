pub mod util;

// Legacy RasterImage (to be deprecated)
pub mod raster_image;

// Phase 1 MVP image command modules (requires image-support feature)
#[cfg(feature = "image-support")]
pub mod io;
#[cfg(feature = "image-support")]
pub mod format;
#[cfg(feature = "image-support")]
pub mod geometric;
#[cfg(feature = "image-support")]
pub mod color;
#[cfg(feature = "image-support")]
pub mod filtering;
#[cfg(feature = "image-support")]
pub mod info;

// Phase 2 image command modules (requires image-support feature)
#[cfg(feature = "image-support")]
pub mod morphology;
#[cfg(feature = "image-support")]
pub mod drawing;
#[cfg(feature = "image-support")]
pub mod edges;

#[cfg(feature = "image-support")]
pub mod commands;
