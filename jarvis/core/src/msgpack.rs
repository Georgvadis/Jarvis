//! Компактный кодек MessagePack для Fish Audio WebSocket Live TTS (`wss://api.fish.audio/v1/tts/live`).
//! Без внешних крейтов: кодирует события клиента (`start`, `text`, `flush`, `stop`) и декодирует
//! серверные события (`audio`, `finish`).

#[derive(Debug, PartialEq, Eq)]
pub enum FishWsEvent {
    Audio(Vec<u8>),
    Finish,
    Other(String),
}

fn push_str(buf: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    let len = b.len();
    if len <= 31 {
        buf.push(0xa0 | (len as u8));
    } else if len <= 0xff {
        buf.push(0xd9);
        buf.push(len as u8);
    } else if len <= 0xffff {
        buf.push(0xda);
        buf.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        buf.push(0xdb);
        buf.extend_from_slice(&(len as u32).to_be_bytes());
    }
    buf.extend_from_slice(b);
}

fn push_u32(buf: &mut Vec<u8>, v: u32) {
    if v <= 127 {
        buf.push(v as u8);
    } else if v <= 0xff {
        buf.push(0xcc);
        buf.push(v as u8);
    } else if v <= 0xffff {
        buf.push(0xcd);
        buf.extend_from_slice(&(v as u16).to_be_bytes());
    } else {
        buf.push(0xce);
        buf.extend_from_slice(&v.to_be_bytes());
    }
}

fn push_fixmap(buf: &mut Vec<u8>, count: u8) {
    debug_assert!(count <= 15);
    buf.push(0x80 | count);
}

/// `{"event":"start","request":{"text":"","format":"pcm","sample_rate":<sr>,"reference_id":<id>,"latency":"balanced","chunk_length":200}}`
pub fn encode_fish_start(reference_id: &str, sample_rate: u32) -> Vec<u8> {
    let mut buf = Vec::with_capacity(160 + reference_id.len());
    push_fixmap(&mut buf, 2);
    push_str(&mut buf, "event");
    push_str(&mut buf, "start");
    push_str(&mut buf, "request");
    push_fixmap(&mut buf, 6);
    push_str(&mut buf, "text");
    push_str(&mut buf, "");
    push_str(&mut buf, "format");
    push_str(&mut buf, "pcm");
    push_str(&mut buf, "sample_rate");
    push_u32(&mut buf, sample_rate);
    push_str(&mut buf, "reference_id");
    push_str(&mut buf, reference_id);
    push_str(&mut buf, "latency");
    push_str(&mut buf, "balanced");
    push_str(&mut buf, "chunk_length");
    push_u32(&mut buf, 200);
    buf
}

/// `{"event":"text","text":"..."}`
pub fn encode_fish_text(text: &str) -> Vec<u8> {
    let mut buf = Vec::with_capacity(32 + text.len());
    push_fixmap(&mut buf, 2);
    push_str(&mut buf, "event");
    push_str(&mut buf, "text");
    push_str(&mut buf, "text");
    push_str(&mut buf, text);
    buf
}

/// `{"event":"flush"}`
pub fn encode_fish_flush() -> Vec<u8> {
    let mut buf = Vec::with_capacity(20);
    push_fixmap(&mut buf, 1);
    push_str(&mut buf, "event");
    push_str(&mut buf, "flush");
    buf
}

/// `{"event":"stop"}`
pub fn encode_fish_stop() -> Vec<u8> {
    let mut buf = Vec::with_capacity(20);
    push_fixmap(&mut buf, 1);
    push_str(&mut buf, "event");
    push_str(&mut buf, "stop");
    buf
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.b.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_be_bytes(self.take(2)?.try_into().ok()?))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }

    fn read_str(&mut self) -> Option<&'a str> {
        let tag = self.u8()?;
        let len = match tag {
            0xa0..=0xbf => (tag & 0x1f) as usize,
            0xd9 => self.u8()? as usize,
            0xda => self.u16()? as usize,
            0xdb => self.u32()? as usize,
            _ => return None,
        };
        std::str::from_utf8(self.take(len)?).ok()
    }

    fn read_bin(&mut self) -> Option<&'a [u8]> {
        let tag = self.u8()?;
        let len = match tag {
            0xc4 => self.u8()? as usize,
            0xc5 => self.u16()? as usize,
            0xc6 => self.u32()? as usize,
            // Some servers encode small buffers as str
            0xa0..=0xbf => (tag & 0x1f) as usize,
            0xd9 => self.u8()? as usize,
            0xda => self.u16()? as usize,
            0xdb => self.u32()? as usize,
            _ => return None,
        };
        self.take(len)
    }

    fn skip_value(&mut self, depth: u8) -> Option<()> {
        if depth > 16 {
            return None;
        }
        let tag = self.u8()?;
        match tag {
            0x00..=0x7f | 0xc0 | 0xc2 | 0xc3 | 0xe0..=0xff => Some(()),
            0x80..=0x8f => {
                let n = (tag & 0x0f) as usize;
                for _ in 0..n {
                    self.skip_value(depth + 1)?;
                    self.skip_value(depth + 1)?;
                }
                Some(())
            }
            0x90..=0x9f => {
                let n = (tag & 0x0f) as usize;
                for _ in 0..n {
                    self.skip_value(depth + 1)?;
                }
                Some(())
            }
            0xa0..=0xbf => {
                let n = (tag & 0x1f) as usize;
                self.take(n).map(|_| ())
            }
            0xc4 | 0xd9 => {
                let n = self.u8()? as usize;
                self.take(n).map(|_| ())
            }
            0xc5 | 0xda => {
                let n = self.u16()? as usize;
                self.take(n).map(|_| ())
            }
            0xc6 | 0xdb => {
                let n = self.u32()? as usize;
                self.take(n).map(|_| ())
            }
            0xc7 => {
                let n = self.u8()? as usize;
                self.take(1 + n).map(|_| ())
            }
            0xc8 => {
                let n = self.u16()? as usize;
                self.take(1 + n).map(|_| ())
            }
            0xc9 => {
                let n = self.u32()? as usize;
                self.take(1 + n).map(|_| ())
            }
            0xca | 0xce | 0xd2 => self.take(4).map(|_| ()),
            0xcb | 0xcf | 0xd3 => self.take(8).map(|_| ()),
            0xcc | 0xd0 => self.take(1).map(|_| ()),
            0xcd | 0xd1 => self.take(2).map(|_| ()),
            0xd4 => self.take(2).map(|_| ()),
            0xd5 => self.take(3).map(|_| ()),
            0xd6 => self.take(5).map(|_| ()),
            0xd7 => self.take(9).map(|_| ()),
            0xd8 => self.take(17).map(|_| ()),
            0xdc => {
                let n = self.u16()? as usize;
                for _ in 0..n {
                    self.skip_value(depth + 1)?;
                }
                Some(())
            }
            0xdd => {
                let n = self.u32()? as usize;
                for _ in 0..n {
                    self.skip_value(depth + 1)?;
                }
                Some(())
            }
            0xde => {
                let n = self.u16()? as usize;
                for _ in 0..n {
                    self.skip_value(depth + 1)?;
                    self.skip_value(depth + 1)?;
                }
                Some(())
            }
            0xdf => {
                let n = self.u32()? as usize;
                for _ in 0..n {
                    self.skip_value(depth + 1)?;
                    self.skip_value(depth + 1)?;
                }
                Some(())
            }
            0xc1 => None,
        }
    }
}

pub fn decode_fish_event(data: &[u8]) -> Option<FishWsEvent> {
    let mut r = Reader { b: data, pos: 0 };
    let tag = r.u8()?;
    let count = match tag {
        0x80..=0x8f => (tag & 0x0f) as usize,
        0xde => r.u16()? as usize,
        0xdf => r.u32()? as usize,
        _ => return None,
    };
    let mut event: Option<String> = None;
    let mut audio: Option<Vec<u8>> = None;
    for _ in 0..count {
        let key = r.read_str()?;
        match key {
            "event" => {
                event = Some(r.read_str()?.to_string());
            }
            "audio" => {
                audio = Some(r.read_bin()?.to_vec());
            }
            _ => {
                r.skip_value(0)?;
            }
        }
    }
    match event.as_deref() {
        Some("audio") => Some(FishWsEvent::Audio(audio.unwrap_or_default())),
        Some("finish") => Some(FishWsEvent::Finish),
        Some(other) => Some(FishWsEvent::Other(other.to_string())),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_client_events_as_valid_msgpack() {
        let start = encode_fish_start("ref-abc-123", 24000);
        assert_eq!(start[0], 0x82); // 2-entry map
        let mut r = Reader { b: &start, pos: 0 };
        assert!(r.skip_value(0).is_some());
        assert_eq!(r.pos, start.len());

        for frame in [encode_fish_text("Привет, мир!"), encode_fish_flush(), encode_fish_stop()] {
            let mut rr = Reader { b: &frame, pos: 0 };
            assert!(rr.skip_value(0).is_some());
            assert_eq!(rr.pos, frame.len());
        }
    }

    #[test]
    fn decodes_server_audio_and_finish_events() {
        // {"event": "audio", "time": 12.5, "audio": <bin16 512 bytes>}
        let payload = vec![0x42u8; 512];
        let mut frame = Vec::new();
        push_fixmap(&mut frame, 3);
        push_str(&mut frame, "event");
        push_str(&mut frame, "audio");
        push_str(&mut frame, "time");
        frame.push(0xcb);
        frame.extend_from_slice(&12.5f64.to_be_bytes());
        push_str(&mut frame, "audio");
        frame.push(0xc5);
        frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        frame.extend_from_slice(&payload);
        assert_eq!(decode_fish_event(&frame), Some(FishWsEvent::Audio(payload)));

        // {"event": "finish", "reason": "stop"}
        let mut fin = Vec::new();
        push_fixmap(&mut fin, 2);
        push_str(&mut fin, "event");
        push_str(&mut fin, "finish");
        push_str(&mut fin, "reason");
        push_str(&mut fin, "stop");
        assert_eq!(decode_fish_event(&fin), Some(FishWsEvent::Finish));
    }
}
