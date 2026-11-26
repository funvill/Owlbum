//! The command line: `trip <folder>`,
//! `large <count> <folder> [--seed <n>] [--megapixels <n>[-<n>]]` and
//! `extras <folder>`.

use std::path::PathBuf;

pub const USAGE: &str = "usage: owlbum-fixtures trip <folder>
       owlbum-fixtures large <count> <folder> [--seed <n>] [--megapixels <n>[-<n>]]
       owlbum-fixtures extras <folder>

The folder must be missing or empty. --megapixels paints the large album
at camera sizes (for example 12-24) instead of tiny ones.";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Trip(PathBuf),
    Large {
        count: usize,
        folder: PathBuf,
        seed: u64,
        /// Camera sizes, `lo..=hi` megapixels; None for the tiny photos.
        megapixels: Option<(u32, u32)>,
    },
    Extras(PathBuf),
}

/// Parse the arguments after the program name.
pub fn parse<I, S>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["trip", folder] => Ok(Command::Trip(folder.into())),
        ["extras", folder] => Ok(Command::Extras(folder.into())),
        ["large", count, folder, rest @ ..] => {
            let count: usize = count
                .parse()
                .map_err(|_| format!("not a count of photos: {count}"))?;
            let mut seed = crate::DEFAULT_SEED;
            let mut megapixels = None;
            for pair in rest.chunks(2) {
                match pair {
                    ["--seed", s] => {
                        seed = s.parse().map_err(|_| format!("not a seed: {s}"))?;
                    }
                    ["--megapixels", m] => megapixels = Some(range(m)?),
                    _ => return Err(USAGE.to_string()),
                }
            }
            Ok(Command::Large {
                count,
                folder: folder.into(),
                seed,
                megapixels,
            })
        }
        _ => Err(USAGE.to_string()),
    }
}

/// `12` or `12-24`, in megapixels.
fn range(text: &str) -> Result<(u32, u32), String> {
    let bad = || format!("not megapixels: {text}");
    let (lo, hi) = text.split_once('-').unwrap_or((text, text));
    let lo: u32 = lo.parse().map_err(|_| bad())?;
    let hi: u32 = hi.parse().map_err(|_| bad())?;
    if lo == 0 || hi < lo || hi > 200 {
        return Err(bad());
    }
    Ok((lo, hi))
}

/// Run a parsed command; returns the line to print.
pub fn run(command: Command) -> std::io::Result<String> {
    let (album, what) = match command {
        Command::Trip(folder) => (crate::trip(&folder)?, "trip"),
        Command::Extras(folder) => (crate::extras(&folder)?, "extras"),
        Command::Large {
            count,
            folder,
            seed,
            megapixels,
        } => (
            crate::large_sized(&folder, count, seed, megapixels)?,
            "large",
        ),
    };
    Ok(format!(
        "wrote the {what} album: {} files in {}",
        album.files.len(),
        album.root.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands() {
        assert_eq!(parse(["trip", "x"]), Ok(Command::Trip("x".into())));
        assert_eq!(parse(["extras", "x"]), Ok(Command::Extras("x".into())));
        assert_eq!(
            parse(["large", "100000", "x"]),
            Ok(Command::Large {
                count: 100_000,
                folder: "x".into(),
                seed: crate::DEFAULT_SEED,
                megapixels: None,
            })
        );
        assert_eq!(
            parse(["large", "5", "x", "--seed", "9"]),
            Ok(Command::Large {
                count: 5,
                folder: "x".into(),
                seed: 9,
                megapixels: None,
            })
        );
        assert_eq!(
            parse(["large", "5", "x", "--megapixels", "12-24", "--seed", "9"]),
            Ok(Command::Large {
                count: 5,
                folder: "x".into(),
                seed: 9,
                megapixels: Some((12, 24)),
            })
        );
        assert_eq!(
            parse(["large", "5", "x", "--megapixels", "20"]),
            Ok(Command::Large {
                count: 5,
                folder: "x".into(),
                seed: crate::DEFAULT_SEED,
                megapixels: Some((20, 20)),
            })
        );
    }

    #[test]
    fn mistakes() {
        assert_eq!(parse(Vec::<String>::new()), Err(USAGE.to_string()));
        assert_eq!(parse(["trip"]), Err(USAGE.to_string()));
        assert_eq!(parse(["trip", "a", "b"]), Err(USAGE.to_string()));
        assert!(parse(["large", "many", "x"]).unwrap_err().contains("many"));
        assert!(parse(["large", "5", "x", "--seed"]).is_err());
        assert!(parse(["large", "5", "x", "--seed", "z"]).is_err());
        assert!(parse(["large", "5", "x", "--megapixels", "24-12"]).is_err());
        assert!(parse(["large", "5", "x", "--megapixels", "0"]).is_err());
        assert!(parse(["large", "5", "x", "--megapixels"]).is_err());
    }

    #[test]
    fn runs() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("big");
        let line = run(Command::Large {
            count: 3,
            folder: folder.clone(),
            seed: 1,
            megapixels: None,
        })
        .unwrap();
        assert!(line.contains("3 files"), "{line}");
        assert!(run(Command::Trip(folder)).is_err(), "folder not empty");
    }
}
