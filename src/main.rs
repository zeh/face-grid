use std::path::PathBuf;

use glob::{GlobError, glob};
use image::{ImageBuffer, Pixel, RgbImage, Rgba, RgbaImage, imageops};
use rust_faces::{
	BlazeFaceParams, FaceDetection, FaceDetectorBuilder, InferParams, Provider, ToArray3, ToRgb8,
};
use structopt::StructOpt;

use geom::{WHf, WHi, XYWHi, XYi, fit_inside, intersect, whf_to_whi, xyf_to_xyi};
use parsing::parse_image_dimensions;

pub mod geom;
pub mod parsing;
pub mod terminal;

/**
 * Copy one image on top of another
 */
fn copy_image(bottom: &mut RgbaImage, top: &RgbImage, cell_top_offset: XYi, cell: XYWHi) {
	// Find paintable intersection between bottom and top
	let bottom_rect = (0, 0, cell.2, cell.3);
	let top_rect = (cell_top_offset.0, cell_top_offset.1, top.width(), top.height());
	let intersection = intersect(bottom_rect, top_rect);
	if intersection.is_none() {
		panic!("Cannot blend image; no intersection between bottom and top image.");
	}
	let intersection_rect = intersection.unwrap();

	let dst_x1 = intersection_rect.0 + cell.0;
	let dst_y1 = intersection_rect.1 + cell.1;
	let dst_x2 = intersection_rect.0 + intersection_rect.2 as i32 + cell.0;
	let dst_y2 = intersection_rect.1 + intersection_rect.3 as i32 + cell.1;

	for dst_y in dst_y1..dst_y2 {
		let src_y = (dst_y - cell_top_offset.1 - cell.1) as u32;
		for dst_x in dst_x1..dst_x2 {
			let src_x = (dst_x - cell_top_offset.0 - cell.0) as u32;
			let top_px: [u8; 3] = top
				.get_pixel(src_x, src_y)
				.channels()
				.to_owned()
				.try_into()
				.expect("converting pixels to array");
			bottom.put_pixel(dst_x as u32, dst_y as u32, Rgba([top_px[0], top_px[1], top_px[2], 255]));
		}
	}
}

#[derive(Debug, StructOpt)]
#[structopt(name = "face-grid", about = "Creates a grid of face-aligned images.")]
struct Opt {
	/// File mask (e.g., "images/*.jpg")
	#[structopt(long, default_value = "*.jpg")]
	input: String,

	/// Output image dimensions (e.g., "800x600")
	#[structopt(long, default_value = "100x100", parse(try_from_str = parse_image_dimensions))]
	cell_size: (u32, u32),

	/// Scale of the face (e.g., "0.5")
	#[structopt(long, default_value = "1")]
	face_scale: f32,

	/// Output file name (e.g., "output.png")
	#[structopt(long, default_value = "face-stack-output.jpg", parse(from_os_str))]
	output: PathBuf,

	/// Number of columns to use in the image. If omitted, try as close as possible to get a square.
	#[structopt(long, default_value = "0")]
	columns: u32,

	/// Number of maximum valid images to use for input
	#[structopt(long, default_value = "0")]
	max_images: u32,
}

fn main() {
	let opt = Opt::from_args();
	let (cell_width, cell_height) = opt.cell_size;

	println!("Will get files from {:?}, and output at {:?}.", opt.input, opt.output);

	let face_detector =
        // Alternative:
        // FaceDetectorBuilder::new(FaceDetection::MtCnn(
        //     MtCnnParams {
        //         min_face_size: 1000,
        //         ..Default::default()
        //     }))
        FaceDetectorBuilder::new(FaceDetection::BlazeFace640(
            BlazeFaceParams {
                // Default is 1280, but finds no images
                // 80 works too
                target_size: 160,
                ..Default::default()
            }))
            .download()
            .infer_params(InferParams {
                provider: Provider::OrtCpu,
                intra_threads: Some(5),
                ..Default::default()
            })
            .build()
            .expect("Failed to load the face detector");

	// Decide where the face will be in the output image
	let typical_face_size: WHf = (75f32, 100f32); // Typically 0.75 aspect ratio
	let faces_rect_inside = fit_inside((cell_width as f32, cell_height as f32), typical_face_size);
	let typical_face_scale = 0.6f32 * opt.face_scale;
	let target_faces_rect: WHf =
		(faces_rect_inside.0 * typical_face_scale, faces_rect_inside.1 * typical_face_scale);

	// First, read all images and find faces, since we have to know how many cells we have in advance
	let mut num_images_read = 0usize;

	// Reads all images from the given input mask
	let image_files = glob(&opt.input)
		.expect(format!("Failed to read glob pattern: {}", opt.input).as_str())
		.collect::<Vec<Result<PathBuf, GlobError>>>();

	let mut results: Vec<(RgbImage, XYi)> = vec![];

	for image_file in &image_files {
		if let Ok(path) = image_file {
			// File can be opened
			terminal::erase_line_to_end();
			print!(
				"(Step 1/2) ({}/{}) Reading {:?}",
				num_images_read + 1,
				image_files.len(),
				&path.file_name().unwrap()
			);

			if let Ok(img) = image::open(&path) {
				// Is a valid image file
				print!(", {:?}x{:?}", img.width(), img.height());
				let array3_image = img.into_rgb8().into_array3();
				let faces = face_detector.detect(array3_image.view().into_dyn()).unwrap();
				print!(", {} faces", faces.len());

				if faces.len() == 1 {
					// Has a valid face
					println!(", confidence {:?}", faces[0].confidence);

					let rgb_image = array3_image.to_rgb8();
					let face_rect = &faces[0].rect;

					// Find out what the face size should be inside our face target box
					let target_face_rect: WHf =
						fit_inside(target_faces_rect, (face_rect.width, face_rect.height));
					let new_image_scale = target_face_rect.0 / face_rect.width;
					let new_image_size: WHi = whf_to_whi((
						rgb_image.width() as f32 * new_image_scale,
						rgb_image.height() as f32 * new_image_scale,
					));

					// Scale the image appropriately
					let resized_image =
						imageops::resize(&rgb_image, new_image_size.0, new_image_size.1, imageops::Lanczos3);

					// Get all the options
					let param_offset: XYi = xyf_to_xyi((
						cell_width as f32 / 2.0 - (face_rect.x + face_rect.width / 2.0) * new_image_scale,
						cell_height as f32 / 2.0 - (face_rect.y + face_rect.height / 2.0) * new_image_scale,
					));

					results.push((resized_image, param_offset));

					terminal::cursor_up();
				} else {
					println!("; no valid faces, skipping.");
				}
			} else {
				println!("; invalid image, skipping.");
			}
		}

		num_images_read += 1;

		if opt.max_images > 0 && results.len() >= opt.max_images as usize {
			terminal::erase_line_to_end();
			println!("Reached the maximum number of input images; skipping additional files.");
			break;
		}
	}

	terminal::erase_line_to_end();
	println!(
		"(Step 1/2) Done. {} images processed, with {} valid results found.",
		image_files.len(),
		results.len()
	);

	let num_cols = if opt.columns == 0 {
		(results.len() as f32).sqrt().ceil() as u32
	} else {
		opt.columns
	};
	let num_rows = (results.len() as f32 / num_cols as f32).ceil() as u32;
	let output_width = num_cols * cell_width;
	let output_height = num_rows * cell_height;

	println!(
		"The output size will be {}x{}, with {} rows and {} columns of images.",
		output_width, output_height, num_rows, num_cols
	);

	// Second, blend the valid images found

	// Create the output image
	let mut output_image: RgbaImage =
		ImageBuffer::from_pixel(output_width, output_height, Rgba([0, 0, 0, 0]));

	let mut num_images_blended = 0;
	for result in &results {
		terminal::erase_line_to_end();
		println!("(Step 2/2) ({}/{}) Blending image", num_images_blended + 1, results.len());
		let col = num_images_blended % num_cols;
		let row = num_images_blended / num_cols;
		let cell_tr = (col * cell_width, row * cell_height);

		copy_image(
			&mut output_image,
			&result.0,
			result.1,
			(cell_tr.0 as i32, cell_tr.1 as i32, cell_width, cell_height),
		);
		num_images_blended += 1;

		terminal::cursor_up();
	}

	terminal::erase_line_to_end();
	println!("(Step 2/2) Done. {} images blended.", results.len());

	// Finally, saved the final image
	output_image.save(&opt.output).expect("Failed to save output image");
}
