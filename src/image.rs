use image::{DynamicImage, GenericImageView, GrayImage, Pixel};
use std::f32;

/// The image type we use in this library.
///
/// This is simply a wrapper around a contiguous f32 vector. A reader might
/// question why we opted for this approach, instead of using the image
/// crate's image type, and in fact I would typically err on the side
/// of of avoiding premature optimization and re-using existing code.
/// I tried just using the image crate's types with f32 as a
/// template argument. All operations were approximately 40% slower. That
/// implementation is in the history of this repository if you're curious.
///
/// The below traits have been violated in various parts of this crate,
/// with some image operations applying directly to the buffer. This,
/// again, ended up being a necessary optimization. Using iterators
/// to perform image filters sped them up in some cases by a factor of
/// 2. Unfortunately this makes the resulting code a bit less readable.
///
/// We continue to use the image crate for loading and saving images.
///
/// There exists the imageproc crate at the time of this writing, that
/// have existing implementations of generalized image convolutions,
/// Gaussian blur, and image resizing. I re-implemented these things here
/// because the image crate versions are missing some key optimizations
/// like using a separable filter, and using the filters implemented
/// here ended up speeding up everything a lot.
#[derive(Debug, Clone)]
pub struct GrayFloatImage {
    pub buffer: Vec<f32>,
    width: usize,
    height: usize,
}
pub trait ImageFunctions {
    /// The width of the image.
    /// # Return value
    /// The width.
    fn width(&self) -> usize;

    /// The height of the image.
    /// # Return value
    /// The height.
    fn height(&self) -> usize;

    /// Create a new image
    ///
    /// # Arguments
    /// * `width` - Width of image
    /// * `height` - Height of image.
    /// # Return value
    /// The image.
    fn new(width: usize, height: usize) -> Self;

    /// Return an image with each dimension halved
    fn half_size(&self) -> Self;

    /// get a float pixel at x, y
    ///
    /// # Arguments
    /// * `x` - x coordinate.
    /// * `y` - y coordinate.
    /// # Return value
    /// the value of the pixel.
    fn get(&self, x: usize, y: usize) -> f32;

    /// put a float pixel to x, y
    ///
    /// # Arguments
    /// * `x` - x coordinate.
    /// * `y` - y coordinate.
    /// pixel_value: value to put
    fn put(&mut self, x: usize, y: usize, pixel_value: f32);
}

impl ImageFunctions for GrayFloatImage {
    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn new(width: usize, height: usize) -> Self {
        Self {
            buffer: vec![0f32; width * height],
            height,
            width,
        }
    }

    fn get(&self, x: usize, y: usize) -> f32 {
        self.buffer[self.width * y + x]
    }

    fn put(&mut self, x: usize, y: usize, pixel_value: f32) {
        self.buffer[self.width * y + x] = pixel_value;
    }
    fn half_size(&self) -> Self {
        let width = self.width() / 2;
        let height = self.height() / 2;
        let mut out = Self::new(width, height);
        for x in 0..width {
            for y in 0..height {
                let mut val = 0f32;
                for x_src in (2 * x)..(2 * x + 2) {
                    for y_src in (2 * y)..(2 * y + 2) {
                        val += self.get(x_src, y_src);
                    }
                }
                out.put(x, y, val / 4f32);
            }
        }
        out
    }
}

/// Create a unit float image from the image crate's DynamicImage type.
///
/// # Arguments
/// * `input_image` - the input image.
/// # Return value
/// An image with pixel values between 0 and 1.
pub fn create_unit_float_image(input_image: &DynamicImage) -> GrayFloatImage {
    let gray_image: GrayImage = input_image.to_luma();
    let mut output_image =
        GrayFloatImage::new(input_image.width() as usize, input_image.height() as usize);
    {
        let mut itr_output = output_image.buffer.iter_mut();
        for gray_pixel in gray_image.pixels() {
            let output_ptr = itr_output.next().unwrap();
            let pixel_value: u8 = gray_pixel.channels()[0];
            *output_ptr = f32::from(pixel_value) * 1f32 / 255f32;
        }
    }
    output_image
}

/// Return sqrt(image_1_i + image_2_i) for all pixels in the input images.
/// Save the result in image_1.
///
/// # Arguments
/// * `image_1` - the first image.
/// * `image_2` - the second image.
pub fn sqrt_squared(image_1: &mut GrayFloatImage, image_2: &GrayFloatImage) {
    debug_assert!(image_1.width() == image_2.width());
    debug_assert!(image_1.height() == image_2.height());
    let length = image_1.width() * image_1.height();
    let slice_1 = &mut image_1.buffer[..];
    let slice_2 = &image_2.buffer[..];
    let mut itr1 = slice_1.iter_mut();
    let mut itr2 = slice_2.iter();
    for _ in 0..(length) {
        let p1 = itr1.next().unwrap();
        let p2 = itr2.next().unwrap();
        *p1 += *p2;
    }
}

/// Fill border with neighboring pixels. A way of preventing instability
/// around the image borders for things like derivatives.
///
/// # Arguments
/// * `output` - the image to operate upon.
/// * `half_width` the number of pixels around the borders to operate on.
pub fn fill_border(output: &mut GrayFloatImage, half_width: usize) {
    for x in 0..output.width() {
        let plus = output.get(x, half_width);
        let minus = output.get(x, output.height() - half_width - 1);
        for y in 0..half_width {
            output.put(x, y, plus);
        }
        for y in (output.height() - half_width)..output.height() {
            output.put(x, y, minus);
        }
    }
    for y in 0..output.height() {
        let plus = output.get(half_width, y);
        let minus = output.get(output.width() - half_width - 1, y);
        for x in 0..half_width {
            output.put(x, y, plus);
        }
        for x in (output.width() - half_width)..output.width() {
            output.put(x, y, minus);
        }
    }
}

/// Horizontal image filter for variable kernel sizes.
///
/// # Arguments
/// * `image` - the input image.
/// * `kernel` the kernel to apply.
/// # Return value
/// The filter result.
#[inline(always)]
pub fn horizontal_filter(image: &GrayFloatImage, kernel: &[f32]) -> GrayFloatImage {
    // Cannot have an even-sized kernel
    debug_assert!(kernel.len() % 2 == 1);
    let half_width = (kernel.len() / 2) as i32;
    let w = image.width() as i32;
    let h = image.height() as i32;
    let mut output = GrayFloatImage::new(image.width(), image.height());
    {
        let out_slice = &mut output.buffer[..];
        let image_slice = &image.buffer[..];
        for k in -half_width..=half_width {
            let mut out_itr = out_slice.iter_mut();
            let mut image_itr = image_slice.iter();
            let mut out_ptr = out_itr.nth(half_width as usize).unwrap();
            let mut image_val = image_itr.nth((half_width + k) as usize).unwrap();
            let kernel_value = kernel[(k + half_width) as usize];
            for _ in half_width..(w * h - half_width - 1) {
                *out_ptr += kernel_value * image_val;
                out_ptr = out_itr.next().unwrap();
                image_val = image_itr.next().unwrap();
            }
        }
    }
    fill_border(&mut output, half_width as usize);
    output
}

/// Vertical image filter for variable kernel sizes.
///
/// # Arguments
/// * `image` - the input image.
/// * `kernel` the kernel to apply.
/// # Return value
/// The filter result.
#[inline(always)]
pub fn vertical_filter(image: &GrayFloatImage, kernel: &[f32]) -> GrayFloatImage {
    // Cannot have an even-sized kernel
    debug_assert!(kernel.len() % 2 == 1);
    let half_width = (kernel.len() / 2) as i32;
    let w = image.width() as i32;
    let h = image.height() as i32;
    let mut output = GrayFloatImage::new(image.width(), image.height());
    {
        let out_slice = &mut output.buffer[..];
        let image_slice = &image.buffer[..];
        for k in -half_width..=half_width {
            let mut out_itr = out_slice.iter_mut();
            let mut image_itr = image_slice.iter();
            let mut out_ptr = out_itr.nth((half_width * w) as usize).unwrap();
            let mut image_val = image_itr
                .nth(((half_width * w) + (k * w)) as usize)
                .unwrap();
            let kernel_value = kernel[(k + half_width) as usize];
            for _ in (half_width * w)..(w * h - (half_width * w) - 1) {
                *out_ptr += kernel_value * image_val;
                out_ptr = out_itr.next().unwrap();
                image_val = image_itr.next().unwrap();
            }
        }
    }
    fill_border(&mut output, half_width as usize);
    output
}

/// The Gaussian function.
///
/// # Arguments
/// * `x` - the offset.
/// * `r` - sigma.
/// # Return value
/// The kernel value at x.
fn gaussian(x: f32, r: f32) -> f32 {
    ((2.0 * f32::consts::PI).sqrt() * r).recip() * (-x.powi(2) / (2.0 * r.powi(2))).exp()
}

/// Generate a Gaussina kernel.
///
/// # Arguments
/// * `r` - sigma.
/// * `kernel_size` - The size of the kernel.
/// # Return value
/// The kernel (a vector).
fn gaussian_kernel(r: f32, kernel_size: usize) -> Vec<f32> {
    let mut kernel = vec![0f32; kernel_size];
    let half_width = (kernel_size / 2) as i32;
    let mut sum = 0f32;
    for i in -half_width..=half_width {
        let val = gaussian(i as f32, r);
        kernel[(i + half_width) as usize] = val;
        sum += val;
    }
    for val in kernel.iter_mut() {
        *val /= sum;
    }
    kernel
}

/// Perform Gaussian blur on an image.
///
/// # Arguments
/// * `r` - sigma.
/// * `kernel_size` - The size of the kernel.
/// # Return value
/// The resulting image after the filter was applied.
pub fn gaussian_blur(image: &GrayFloatImage, r: f32) -> GrayFloatImage {
    // a separable Gaussian kernel
    let kernel_size = (f32::ceil(r) as usize) * 2 + 1usize;
    let kernel = gaussian_kernel(r, kernel_size);
    let img_horizontal = horizontal_filter(&image, &kernel);
    vertical_filter(&img_horizontal, &kernel)
}

#[cfg(test)]
mod tests {
    use super::gaussian_kernel;
    #[test]
    fn gaussian_kernel_correct() {
        // test against known correct kernel
        let kernel = gaussian_kernel(3.0, 7);
        let known_correct_kernel = vec![
            0.1062_8852,
            0.1403_2133,
            0.1657_7007,
            0.1752_4014,
            0.1657_7007,
            0.1403_2133,
            0.1062_8852,
        ];
        for it in kernel.iter().zip(known_correct_kernel.iter()) {
            let (i, j) = it;
            assert!(f32::abs(*i - *j) < 0.0001);
        }
    }
}