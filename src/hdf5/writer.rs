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
    max_buffer_size: usize, // Maximum allowed buffer size to prevent memory bloat
    current_length: usize,
    channel_format: lsl::ChannelFormat,
    last_flush_time: Instant,
    flush_interval: Duration,
    // Pre-allocated buffer to avoid allocations during flush
    temp_data_buffer: Vec<f64>, // Use f64 as largest type, cast as needed
    // Backpressure monitoring
    slow_flush_warnings: u32,
    last_flush_duration: Duration,
}

impl Hdf5Writer {
    pub fn new(
        data_dataset: Dataset,
        time_dataset: Dataset,
        buffer_size: usize,
        channel_format: lsl::ChannelFormat,
        flush_interval: Duration,
    ) -> Result<Self> {
        // Set max buffer size to 10x normal buffer size to prevent memory bloat
        let max_buffer_size = (buffer_size * 10).max(1000);
        let current_length = data_dataset.shape()[1]; // Second dimension is now time
        Ok(Self {
            data_dataset,
            time_dataset,
            sample_buffer: Vec::new(),
            time_buffer: Vec::new(),
            buffer_size,
            max_buffer_size,
            current_length,
            channel_format,
            last_flush_time: Instant::now(),
            flush_interval,
            temp_data_buffer: Vec::new(),
            slow_flush_warnings: 0,
            last_flush_duration: Duration::from_millis(0),
        })
    }

    /// Add sample by reference to avoid cloning - more efficient for hot path
    pub fn add_sample_slice_f32(&mut self, data: &[f32], timestamp: f64) {
        self.sample_buffer.push(SampleData::Float32(data.to_vec()));
        self.time_buffer.push(timestamp);
    }

    pub fn add_sample_slice_f64(&mut self, data: &[f64], timestamp: f64) {
        self.sample_buffer.push(SampleData::Float64(data.to_vec()));
        self.time_buffer.push(timestamp);
    }

    pub fn add_sample_slice_i32(&mut self, data: &[i32], timestamp: f64) {
        self.sample_buffer.push(SampleData::Int32(data.to_vec()));
        self.time_buffer.push(timestamp);
    }

    pub fn add_sample_slice_i16(&mut self, data: &[i16], timestamp: f64) {
        self.sample_buffer.push(SampleData::Int16(data.to_vec()));
        self.time_buffer.push(timestamp);
    }

    pub fn add_sample_slice_i8(&mut self, data: &[i8], timestamp: f64) {
        self.sample_buffer.push(SampleData::Int8(data.to_vec()));
        self.time_buffer.push(timestamp);
    }

    pub fn add_sample_slice_string(&mut self, data: &[String], timestamp: f64) {
        self.sample_buffer.push(SampleData::String(data.to_vec()));
        self.time_buffer.push(timestamp);
    }

    pub fn flush(&mut self) -> Result<()> {
        if self.sample_buffer.is_empty() {
            return Ok(());
        }

        let flush_start = Instant::now();

        let num_samples = self.sample_buffer.len();
        let num_channels = self.sample_buffer[0].len();
        let new_length = self.current_length + num_samples;

        // Resize datasets to accommodate new data
        self.data_dataset.resize((num_channels, new_length))?;
        self.time_dataset.resize(new_length)?;

        // Prepare time as 1D array - move data to avoid clone
        let time_array = Array1::from_vec(std::mem::take(&mut self.time_buffer));

        // Write data based on channel format using write_slice - reuse temp buffer
        macro_rules! write_samples {
            ($type:ty, $variant:ident) => {{
                // Prepare flattened data buffer
                self.temp_data_buffer.clear();
                self.temp_data_buffer.reserve(num_channels * num_samples);

                // Fill buffer in column-major order (channel-first layout for HDF5)
                for channel in 0..num_channels {
                    for i in 0..num_samples {
                        if let SampleData::$variant(values) = &self.sample_buffer[i] {
                            self.temp_data_buffer.push(values[channel] as f64);
                        }
                    }
                }

                // Cast to target type and create array
                let typed_data: Vec<$type> =
                    self.temp_data_buffer.iter().map(|&x| x as $type).collect();
                let data_array =
                    Array2::<$type>::from_shape_vec((num_channels, num_samples), typed_data)?;

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

        // Flush datasets to ensure data is written to disk
        self.data_dataset.file()?.flush()?;

        // Monitor flush performance and detect backpressure
        let flush_duration = flush_start.elapsed();
        self.last_flush_duration = flush_duration;
        self.last_flush_time = Instant::now();

        // Warn about slow flushes that might indicate backpressure
        if flush_duration > Duration::from_millis(100) {
            self.slow_flush_warnings += 1;
            if self.slow_flush_warnings <= 5 {
                // Only warn first 5 times
                println!(
                    "Warning: Slow HDF5 flush detected:\t{:.1}ms for {} samples (warning {}/5)",
                    flush_duration.as_millis(),
                    num_samples,
                    self.slow_flush_warnings
                );
            }
        }

        if self.slow_flush_warnings <= 5 {
            println!(
                "HDF5: Wrote {} samples (total: {} samples, {:.1}ms flush)",
                num_samples,
                self.current_length,
                flush_duration.as_millis()
            );
        }

        Ok(())
    }

    pub fn needs_flush(&self) -> bool {
        // Force flush if approaching memory limit (emergency flush)
        if self.sample_buffer.len() >= self.max_buffer_size {
            return true;
        }

        // Check buffer size threshold
        if self.sample_buffer.len() >= self.buffer_size {
            return true;
        }

        // Check time-based threshold (only if we have samples to flush)
        if !self.sample_buffer.is_empty() && self.last_flush_time.elapsed() >= self.flush_interval {
            return true;
        }

        // Force flush if we're accumulating samples faster than we can write (backpressure)
        if self.sample_buffer.len() > self.buffer_size / 2
            && self.last_flush_duration > Duration::from_millis(50)
        {
            return true;
        }

        false
    }

    /// Get current buffer sample count for monitoring
    pub fn buffer_sample_count(&self) -> usize {
        self.sample_buffer.len()
    }

    /// Get buffer capacity for monitoring
    pub fn buffer_capacity(&self) -> usize {
        self.max_buffer_size
    }
}
