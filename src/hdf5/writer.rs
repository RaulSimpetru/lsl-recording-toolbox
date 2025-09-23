use anyhow::Result;
use hdf5::Dataset;
use ndarray::{Array1, Array2};
use std::time::{Duration, Instant};

/// Enum to handle different LSL data types
#[derive(Debug, Clone)]
pub enum SampleData {
    Float32(Vec<f32>),
    Float64(Vec<f64>),
    Int32(Vec<i32>),
    Int16(Vec<i16>),
    Int8(Vec<i8>),
    String(Vec<String>),
}

impl SampleData {
    pub fn len(&self) -> usize {
        match self {
            SampleData::Float32(v) => v.len(),
            SampleData::Float64(v) => v.len(),
            SampleData::Int32(v) => v.len(),
            SampleData::Int16(v) => v.len(),
            SampleData::Int8(v) => v.len(),
            SampleData::String(v) => v.len(),
        }
    }
}

/// Structure to manage HDF5 writing with buffering
pub struct Hdf5Writer {
    data_dataset: Dataset,
    time_dataset: Dataset,
    sample_buffer: Vec<SampleData>,
    time_buffer: Vec<f64>,
    buffer_size: usize,
    current_length: usize,
    channel_format: lsl::ChannelFormat,
    last_flush_time: Instant,
    flush_interval: Duration,
}

impl Hdf5Writer {
    pub fn new(
        data_dataset: Dataset,
        time_dataset: Dataset,
        buffer_size: usize,
        channel_format: lsl::ChannelFormat,
        flush_interval: Duration,
    ) -> Result<Self> {
        let current_length = data_dataset.shape()[1]; // Second dimension is now time
        Ok(Self {
            data_dataset,
            time_dataset,
            sample_buffer: Vec::new(),
            time_buffer: Vec::new(),
            buffer_size,
            current_length,
            channel_format,
            last_flush_time: Instant::now(),
            flush_interval,
        })
    }

    pub fn add_sample(&mut self, sample: SampleData, timestamp: f64) {
        self.sample_buffer.push(sample);
        self.time_buffer.push(timestamp);
    }

    pub fn flush(&mut self) -> Result<()> {
        if self.sample_buffer.is_empty() {
            return Ok(());
        }

        let num_samples = self.sample_buffer.len();
        let num_channels = self.sample_buffer[0].len();
        let new_length = self.current_length + num_samples;

        // Resize datasets to accommodate new data
        self.data_dataset.resize((num_channels, new_length))?;
        self.time_dataset.resize(new_length)?;

        // Prepare time as 1D array
        let time_array = Array1::from_vec(self.time_buffer.clone());

        // Write data based on channel format using write_slice
        macro_rules! write_samples {
            ($type:ty, $variant:ident) => {{
                let mut data_array = Array2::<$type>::zeros((num_channels, num_samples));
                for (i, sample) in self.sample_buffer.iter().enumerate() {
                    if let SampleData::$variant(values) = sample {
                        for (j, &value) in values.iter().enumerate() {
                            data_array[[j, i]] = value; // j is channel, i is time
                        }
                    }
                }
                self.data_dataset
                    .write_slice(&data_array, (.., self.current_length..new_length))?;
            }};
        }

        match self.channel_format {
            lsl::ChannelFormat::Float32 => write_samples!(f32, Float32),
            lsl::ChannelFormat::Double64 => write_samples!(f64, Float64),
            lsl::ChannelFormat::Int32 => write_samples!(i32, Int32),
            lsl::ChannelFormat::Int16 => write_samples!(i16, Int16),
            lsl::ChannelFormat::Int8 => write_samples!(i8, Int8),
            _ => {
                return Err(anyhow::anyhow!(
                    "String format not yet implemented for HDF5"
                ));
            }
        }

        // Write time data to the specific slice
        self.time_dataset
            .write_slice(&time_array, self.current_length..new_length)?;

        self.current_length = new_length;
        self.sample_buffer.clear();
        self.time_buffer.clear();

        // Update last flush time
        self.last_flush_time = Instant::now();

        // Flush datasets to ensure data is written to disk
        self.data_dataset.file()?.flush()?;

        println!(
            "HDF5: Wrote {} samples (total: {} samples) - {:?}",
            num_samples, self.current_length, self.channel_format
        );

        Ok(())
    }

    pub fn needs_flush(&self) -> bool {
        // Check buffer size threshold
        if self.sample_buffer.len() >= self.buffer_size {
            return true;
        }

        // Check time-based threshold (only if we have samples to flush)
        if !self.sample_buffer.is_empty() && self.last_flush_time.elapsed() >= self.flush_interval {
            return true;
        }

        false
    }
}
