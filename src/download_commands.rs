#[derive(Debug, Eq, PartialEq)]
pub enum DownloadCommand {
    ShuffleQueue,
    ShuffleAll,
    Clear,
}

impl DownloadCommand {
    pub fn parse(input: &str) -> Result<Self, String> {
        match input.split_whitespace().collect::<Vec<_>>().as_slice() {
            [":shuffle", "queue"] => Ok(Self::ShuffleQueue),
            [":shuffle", "all"] => Ok(Self::ShuffleAll),
            [":clear"] => Ok(Self::Clear),
            _ => Err("Commands: :shuffle queue · :shuffle all · :clear".to_string()),
        }
    }

    pub fn execute(
        self,
        library: &[(String, String)],
        queue: &mut Vec<(String, String)>,
    ) -> String {
        match self {
            Self::ShuffleQueue => {
                fastrand::shuffle(queue);
                format!("Shuffled {} queued song(s).", queue.len())
            }
            Self::ShuffleAll => {
                queue.extend(library.iter().cloned());
                fastrand::shuffle(queue);
                format!("Added and shuffled {} library song(s).", library.len())
            }
            Self::Clear => {
                let removed = queue.len();
                queue.clear();
                format!("Cleared {removed} queued song(s).")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DownloadCommand;

    #[test]
    fn parses_supported_commands_with_flexible_spacing() {
        assert_eq!(
            DownloadCommand::parse(" :shuffle   queue ").unwrap(),
            DownloadCommand::ShuffleQueue
        );
        assert_eq!(
            DownloadCommand::parse(":shuffle all").unwrap(),
            DownloadCommand::ShuffleAll
        );
        assert_eq!(
            DownloadCommand::parse(":clear").unwrap(),
            DownloadCommand::Clear
        );
    }

    #[test]
    fn shuffle_all_adds_every_library_track() {
        let library = vec![
            ("One".to_string(), "one.mp3".to_string()),
            ("Two".to_string(), "two.mp3".to_string()),
        ];
        let mut queue = vec![("Existing".to_string(), "existing.mp3".to_string())];
        DownloadCommand::ShuffleAll.execute(&library, &mut queue);
        queue.sort();
        let mut expected = library;
        expected.push(("Existing".to_string(), "existing.mp3".to_string()));
        expected.sort();
        assert_eq!(queue, expected);
    }

    #[test]
    fn clear_empties_the_queue() {
        let mut queue = vec![("One".to_string(), "one.mp3".to_string())];
        DownloadCommand::Clear.execute(&[], &mut queue);
        assert!(queue.is_empty());
    }
}
