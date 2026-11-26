//! The command line: `trip <folder>`, `large <count> <folder> [--seed <n>]`
//! and `extras <folder>`.

use std::path::PathBuf;

pub const USAGE: &str = "usage: owlbum-fixtures trip <folder>
       owlbum-fixtures large <count> <folder> [--seed <n>]
       owlbum-fixtures extras <folder>

The folder must be missing or empty.";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Trip(PathBuf),
    Large {
        count: usize,
        folder: PathBuf,
        seed: u64,
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
            let seed = match rest {
                [] => crate::DEFAULT_SEED,
                ["--seed", s] => s.parse().map_err(|_| format!("not a seed: {s}"))?,
                _ => return Err(USAGE.to_string()),
            };
            Ok(Command::Large {
                count,
                folder: folder.into(),
                seed,
            })
        }
        _ => Err(USAGE.to_string()),
    }
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
        } => (crate::large(&folder, count, seed)?, "large"),
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
                seed: crate::DEFAULT_SEED
            })
        );
        assert_eq!(
            parse(["large", "5", "x", "--seed", "9"]),
            Ok(Command::Large {
                count: 5,
                folder: "x".into(),
                seed: 9
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
    }

    #[test]
    fn runs() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("big");
        let line = run(Command::Large {
            count: 3,
            folder: folder.clone(),
            seed: 1,
        })
        .unwrap();
        assert!(line.contains("3 files"), "{line}");
        assert!(run(Command::Trip(folder)).is_err(), "folder not empty");
    }
}
