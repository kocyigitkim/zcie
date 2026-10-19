//! A model2vec static embedding model: one vector per token; a text vector is the normalized
//! mean of its token vectors (https://github.com/MinishLab/model2vec). No neural network runs.
//!
//! The token vectors are stored as int8 with one scale per row, so a large multilingual model
//! (500,000 tokens) uses about a quarter of its file size in memory. The file is read in a
//! stream; it is never in memory as a whole.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use serde_json::Value;

use crate::error::{Error, Result};
use crate::tokenize::Tokenizer;

/// Name of the embedding tensor in `model.safetensors`.
const TENSOR: &str = "embeddings";
/// Most tokens of one text (the rest is ignored).
const MAX_TOKENS: usize = 512;
/// Largest int8 value of a quantized row.
const QUANT_MAX: f32 = 127.0;

/// A loaded static model.
pub struct StaticModel {
    tokenizer: Tokenizer,
    dim: usize,
    rows: Vec<i8>,
    scales: Vec<f32>,
}

impl StaticModel {
    /// Loads `tokenizer.json` (WordPiece or Unigram) and `model.safetensors` (F32 `[vocab, dim]`).
    pub fn load(folder: &Path) -> Result<StaticModel> {
        let json: Value = serde_json::from_slice(&std::fs::read(folder.join("tokenizer.json"))?)
            .map_err(|e| Error::Format(format!("tokenizer.json: {e}")))?;
        let tokenizer = Tokenizer::from_json(&json)?;
        let (count, dim, rows, scales) = read_tensor(&folder.join("model.safetensors"))?;
        if count < tokenizer.len() {
            return Err(Error::Format(format!("{count} vectors for {} tokens", tokenizer.len())));
        }
        Ok(StaticModel { tokenizer, dim, rows, scales })
    }

    /// Returns the vector size.
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Returns the unit vector of a text, or `None` if no token is known.
    pub fn embed(&self, text: &str) -> Option<Vec<f32>> {
        let ids = self.tokenizer.encode(text);
        if ids.is_empty() {
            return None;
        }
        let mut sum = vec![0.0_f32; self.dim];
        for id in ids.into_iter().take(MAX_TOKENS) {
            let index = id as usize;
            let row = &self.rows[index * self.dim..(index + 1) * self.dim];
            let scale = self.scales[index];
            sum.iter_mut().zip(row).for_each(|(s, v)| *s += f32::from(*v) * scale);
        }
        normalize(sum)
    }
}

fn normalize(mut vector: Vec<f32>) -> Option<Vec<f32>> {
    let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm <= f32::EPSILON {
        return None;
    }
    vector.iter_mut().for_each(|v| *v /= norm);
    Some(vector)
}

/// Reads the safetensors header: (rows, columns, data start after the header).
fn read_header(reader: &mut impl Read) -> Result<(usize, usize, usize)> {
    let format = |m: &str| Error::Format(format!("model.safetensors: {m}"));
    let mut len = [0u8; 8];
    reader.read_exact(&mut len)?;
    let header_len = usize::try_from(u64::from_le_bytes(len)).map_err(|_| format("header size"))?;
    let mut header = vec![0u8; header_len];
    reader.read_exact(&mut header)?;
    let header: Value = serde_json::from_slice(&header).map_err(|e| format(&e.to_string()))?;
    let tensor = &header[TENSOR];
    if tensor["dtype"].as_str() != Some("F32") {
        return Err(format("the tensor must be F32"));
    }
    let numbers = |key: &str| -> Vec<usize> { tensor[key].as_array().into_iter().flatten().filter_map(Value::as_u64).map(|v| v as usize).collect() };
    let (&[rows, dim], &[start, end]) = (numbers("shape").as_slice(), numbers("data_offsets").as_slice()) else { return Err(format("bad shape or offsets")) };
    if end.checked_sub(start) != Some(rows * dim * 4) {
        return Err(format("data size does not match the shape"));
    }
    Ok((rows, dim, start))
}

/// Reads the F32 tensor row by row and stores each row as int8 with its scale.
fn read_tensor(path: &Path) -> Result<(usize, usize, Vec<i8>, Vec<f32>)> {
    let mut reader = BufReader::new(File::open(path)?);
    let (rows, dim, start) = read_header(&mut reader)?;
    std::io::copy(&mut (&mut reader).take(start as u64), &mut std::io::sink())?;
    let mut quantized = Vec::with_capacity(rows * dim);
    let mut scales = Vec::with_capacity(rows);
    let mut bytes = vec![0u8; dim * 4];
    for _ in 0..rows {
        reader.read_exact(&mut bytes)?;
        let row: Vec<f32> = bytes.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect();
        let peak = row.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        let scale = if peak > 0.0 { peak / QUANT_MAX } else { 1.0 };
        quantized.extend(row.iter().map(|v| (v / scale).round().clamp(-QUANT_MAX, QUANT_MAX) as i8));
        scales.push(scale);
    }
    Ok((rows, dim, quantized, scales))
}
