use std::path::Path;

use owlbum_fixtures::encode::{Meta, jpeg};
use owlbum_fixtures::picture::scene;

use super::*;

fn field<'a>(fields: &'a [ExifField], name: &str) -> Option<&'a ExifField> {
    fields.iter().find(|f| f.name == name)
}

fn write(dir: &Path, name: &str, meta: &Meta) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, jpeg(&scene(7, 64, 48), 85, meta)).unwrap();
    path
}

#[test]
fn lists_every_tag_of_the_file_with_names_groups_and_readable_values() {
    let tmp = tempfile::tempdir().unwrap();
    let meta = Meta {
        make: Some("Owlbum"),
        model: Some("Fixture One"),
        taken: Some("2024:06:12 14:03:00".into()),
        exposure: Some([1, 200]),
        f_number_x10: Some(56),
        iso: Some(100),
        focal_mm: Some(35),
        gps: Some((48_858_370, 2_294_480)),
        ..Meta::default()
    };
    let path = write(tmp.path(), "tagged.jpg", &meta);
    let fields = exif_fields(&path, "jpeg");

    let make = field(&fields, "Make").unwrap();
    assert_eq!(
        (make.group.as_str(), make.value.as_str()),
        ("Image", "Owlbum")
    );
    assert_eq!(field(&fields, "Model").unwrap().value, "Fixture One");
    let exposure = field(&fields, "ExposureTime").unwrap();
    assert_eq!(
        (exposure.group.as_str(), exposure.value.as_str()),
        ("Exif", "1/200")
    );
    assert_eq!(field(&fields, "FNumber").unwrap().value, "56/10");
    assert_eq!(
        field(&fields, "ISOSpeedRatings").map_or("", |f| f.value.as_str()),
        "100"
    );
    assert_eq!(field(&fields, "FocalLength").unwrap().value, "35");
    // A tag of type UNDEFINED that holds text reads as that text.
    assert_eq!(field(&fields, "ExifVersion").unwrap().value, "0232");
    let lat = field(&fields, "GPSLatitudeRef").unwrap();
    assert_eq!((lat.group.as_str(), lat.value.as_str()), ("GPS", "N"));
    assert!(
        field(&fields, "GPSLatitude")
            .unwrap()
            .value
            .starts_with("48, 51, ")
    );
    assert!(
        fields
            .iter()
            .all(|f| !f.name.is_empty() && f.value.chars().count() <= MAX_CHARS + 1)
    );
}

#[test]
fn a_file_without_exif_or_that_cannot_be_read_gives_an_empty_list() {
    let tmp = tempfile::tempdir().unwrap();
    let plain = write(tmp.path(), "plain.jpg", &Meta::default());
    assert_eq!(exif_fields(&plain, "jpeg"), Vec::new());
    assert_eq!(
        exif_fields(&tmp.path().join("gone.jpg"), "jpeg"),
        Vec::new()
    );
}

#[test]
fn values_read_as_text() {
    assert_eq!(bytes_text(b"0232"), "0232");
    assert_eq!(bytes_text(b"ASCII\0\0\0"), "ASCII");
    assert_eq!(bytes_text(&[1, 2, 255]), "01 02 FF");
    let long = vec![0xAB; 40];
    assert!(bytes_text(&long).ends_with("… (40 bytes)"));
    assert_eq!(
        value_text(&Value::Ratios(vec![(1, 200), (35, 1)])),
        "1/200, 35"
    );
    assert_eq!(code_name(0x1234), "Tag 0x1234");
    let cut_text = cut("x".repeat(MAX_CHARS + 5));
    assert_eq!(cut_text.chars().count(), MAX_CHARS + 1);
    assert!(cut_text.ends_with('…'));
}
