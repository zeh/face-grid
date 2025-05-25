// Originally (partly) from https://github.com/zeh/random-art-generator/blob/main/src/generator/utils/parsing.rs

fn parse_integer(src: &str) -> Result<u32, &str> {
	src.parse::<u32>().or(Err("Could not parse integer value"))
}

fn parse_integer_list(src: &str, divider: char) -> Result<Vec<u32>, &str> {
	src.split(divider).collect::<Vec<&str>>().iter().map(|&e| parse_integer(e)).collect()
}

/// Parses a dimensions string (999x999) into a (u32, u32) width/height tuple.
pub fn parse_image_dimensions(src: &str) -> Result<(u32, u32), &str> {
	let values = parse_integer_list(&src, 'x')?;
	match values.len() {
		2 => Ok((values[0], values[1])),
		_ => Err("Dimensions should use WIDTHxHEIGHT"),
	}
}
