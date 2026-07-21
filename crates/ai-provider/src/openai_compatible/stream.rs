use crate::ModelError;

#[derive(Debug, Default)]
pub(crate) struct SseParser {
    buffer: Vec<u8>,
}

impl SseParser {
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>, ModelError> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some((block_end, delimiter_end)) = find_event_boundary(&self.buffer) {
            let block = String::from_utf8(self.buffer[..block_end].to_vec()).map_err(|_| {
                ModelError::IncompatibleResponse("stream is not valid UTF-8".to_owned())
            })?;
            self.buffer.drain(..delimiter_end);
            if let Some(data) = parse_block(&block) {
                events.push(data);
            }
        }
        Ok(events)
    }

    pub(crate) fn finish(&mut self) -> Result<Vec<String>, ModelError> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }
        let block = String::from_utf8(std::mem::take(&mut self.buffer)).map_err(|_| {
            ModelError::IncompatibleResponse("stream is not valid UTF-8".to_owned())
        })?;
        Ok(parse_block(&block).into_iter().collect())
    }
}

fn find_event_boundary(bytes: &[u8]) -> Option<(usize, usize)> {
    for index in 0..bytes.len() {
        if bytes.get(index..index + 2) == Some(b"\n\n") {
            return Some((index, index + 2));
        }
        if bytes.get(index..index + 4) == Some(b"\r\n\r\n") {
            return Some((index, index + 4));
        }
    }
    None
}

fn parse_block(block: &str) -> Option<String> {
    let data = block
        .lines()
        .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
        .collect::<Vec<_>>()
        .join("\n");
    (!data.is_empty()).then_some(data)
}
