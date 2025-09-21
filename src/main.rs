use anyhow::Result;
use clap::Parser;
use core::time;
use lsl::Pullable;
use std::io;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lsl-recorder")]
#[command(about = "Record LSL streams to disk with dedicated control interface")]
pub struct Args {
    #[arg(long, help = "LSL stream source ID to record", default_value = "1234")]
    pub source_id: String,

    #[arg(
        long,
        short = 'o',
        help = "Output file path (XDF format)",
        default_value = "output.xdf"
    )]
    pub output: PathBuf,

    #[arg(
        long,
        help = "Named pipe for control commands (auto-generated if not specified)"
    )]
    pub pipe_name: Option<String>,

    #[arg(long, short = 'd', help = "Maximum recording duration in seconds")]
    pub duration: Option<u64>,

    #[arg(long, default_value = "1000", help = "Stream buffer size")]
    pub buffer_size: usize,

    #[arg(long, short = 'q', help = "Minimal output mode")]
    pub quiet: bool,

    #[arg(
        long,
        default_value = "5.0",
        help = "Timeout for stream resolution in seconds"
    )]
    pub resolve_timeout: f64,
}

fn connect_to_lsl(source_id: &str, timeout: f64) -> Result<(), lsl::Error> {
    println!("Resolving stream...");

    let res = lsl::resolve_bypred(&format!("source_id={}", source_id), 1, timeout)?;

    // Next we're creating an inlet to read from it. Let's say this is a real-time processing tool,
    // and we have no use for more than 10 seconds of data backlog accumulating in case our program
    // stalls for a while, so we set the max buffer length to 10s. We'll also ask that the data be
    // transmitted in chunks of 8 samples at a time, e.g., to save network bandwidth.
    let inl = lsl::StreamInlet::new(&res[0], 360, 0, true)?;

    // now that we have the inlet we can use it to retrieve the full StreamInfo object from it
    // (since custom meta-data could in theory be gigabytes, this is not transmitted by the resolve
    // call)
    let mut info = inl.info(timeout)?;

    // we can now traverse the extended meta-data of the stream to get the information we need
    // (usually we'll want at least the channel labels, which are typically stored as below)
    println!("\nThe channel labels were:");
    let mut cursor = info.desc().child("channels").child("channel");
    while cursor.is_valid() {
        print!("  {}", cursor.child_value_named("label"));
        cursor = cursor.next_sibling();
    }
    // ... alternatively we could get an XML string and parse it using some other tool
    println!("\n\nThe StreamInfo's full XML dump is: {}", info.to_xml()?);

    println!("Press [Enter] to continue");
    let mut ret = String::new();
    io::stdin().read_line(&mut ret).expect("stdin read error");

    
    let channel_format = info.channel_format();
    println!("Channel format is: {:?}", channel_format);
    // in this example we only handle float32 data

    // autoconvert the stream to what

    // let's also suppose that we want to sync the received data's time stamps with our local_clock(),
    // e.g., to relate the data to some local events. We can enable that via post-processing, but
    // see also the inlet's time_correction() method for the manual way that gives you full control
    inl.set_postprocessing(&[
        lsl::ProcessingOption::ClockSync,
        lsl::ProcessingOption::Dejitter,
        lsl::ProcessingOption::Threadsafe,
    ])?;

    // now we're reading data in a loop and print it as we go
    println!("Reading data...");
    let mut sample = Vec::<f32>::new();
    loop {
        // do a blocking read (with finite timeout) to get the next successive sample and its
        // time stamp; we read into a pre-allocated buffer (which will be right-sized by the pull
        // call) since that's more efficient for high-bandwidth data.
        let ts = inl.pull_sample_buf(&mut sample, timeout)?;

        // if we're using a finite timeout we need to check if the timestamp is nonzero (zero means
        // no new data)
        if ts != 0.0 {
            println!("got {:?} at time {}", sample, ts);
        } else {
            println!("got no new data, waiting some more...")
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if !args.quiet {
        tracing_subscriber::fmt::init();
    }

    tracing::info!("Starting LSL recorder for source ID: {}", args.source_id);
    // tracing::info!("Output file: {}", args.output.display());

    connect_to_lsl(&args.source_id, args.resolve_timeout)?;
    Ok(())
}
