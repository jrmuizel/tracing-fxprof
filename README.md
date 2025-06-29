# tracing-fxprof

A [tracing](https://github.com/tokio-rs/tracing) subscriber that outputs profiles in the [Firefox Profiler](https://profiler.firefox.com/) format.

This crate provides `FxProfSubscriber`, a `tracing::Subscriber` implementation that captures tracing spans and events and converts them into a profile format that can be viewed in the Firefox Profiler web interface.

## Features

- **Span Tracking**: Automatically converts tracing spans into call stack frames
- **Event Markers**: Converts tracing events into timeline markers
- **Multi-threaded Support**: Handles spans and events across multiple threads
- **Firefox Profiler Compatible**: Outputs profiles in the processed profile JSON format
- **Easy Integration**: Drop-in replacement for other tracing subscribers

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
tracing-fxprof = "0.1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
```

## Usage

### Basic Usage

```rust
use tracing_fxprof::FxProfSubscriber;
use tracing_subscriber::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create the subscriber
    let subscriber = FxProfSubscriber::new("my-application");
    
    // Initialize tracing
    let _guard = tracing_subscriber::registry()
        .with(subscriber.clone())
        .set_default();
    
    // Your application code with tracing
    {
        let _span = tracing::info_span!("main_function").entered();
        tracing::info!("Application started");
        
        {
            let _span = tracing::debug_span!("inner_function").entered();
            tracing::debug!("Processing data");
        }
        
        tracing::info!("Application finished");
    }
    
    // Save the profile
    subscriber.save_profile("profile.json")?;
    println!("Profile saved! View it at https://profiler.firefox.com");
    
    Ok(())
}
```

### Using with `#[tracing::instrument]`

The subscriber works great with the `#[tracing::instrument]` attribute:

```rust
use tracing::instrument;

#[instrument]
fn fibonacci(n: u64) -> u64 {
    if n <= 1 {
        tracing::debug!("Base case: n = {}", n);
        n
    } else {
        tracing::debug!("Calculating fibonacci({})", n);
        fibonacci(n - 1) + fibonacci(n - 2)
    }
}

#[instrument]
async fn async_work(id: u32) {
    tracing::info!("Starting async work {}", id);
    // ... async work here ...
    tracing::info!("Finished async work {}", id);
}
```

## Viewing Profiles

1. Run your application with the `FxProfSubscriber`
2. Save the profile using `subscriber.save_profile("profile.json")`
3. Open [https://profiler.firefox.com](https://profiler.firefox.com) in your browser
4. Drag and drop the `profile.json` file onto the web page
5. Explore your application's performance profile!

## Profile Structure

The generated profiles include:

- **Call Tree**: Shows the hierarchy of function calls based on your tracing spans
- **Timeline**: Displays when spans were active and events occurred
- **Markers**: Events appear as markers on the timeline with associated metadata
- **Multi-thread Support**: Each thread gets its own timeline and call tree

## Examples

Run the included example:

```bash
cargo run --example basic_usage
```

This will generate a `profile.json` file that you can load into the Firefox Profiler.

## API Reference

### FxProfSubscriber

#### `new(product_name: &str) -> Self`

Creates a new `FxProfSubscriber` with the given product name.

#### `export_profile() -> Result<String, serde_json::Error>`

Exports the current profile as a JSON string in Firefox Profiler format.

#### `save_profile<P: AsRef<Path>>(path: P) -> Result<(), Box<dyn std::error::Error>>`

Saves the profile to a file.

## How It Works

The subscriber implements the `tracing-subscriber::Layer` trait and:

1. **On Span Creation**: Records span metadata (name, target, level)
2. **On Span Enter**: Creates a stack frame and adds a sample to the profile
3. **On Span Exit**: Pops the stack frame and adds another sample
4. **On Events**: Creates timeline markers with the event information
5. **Thread Handling**: Automatically creates thread entries for multi-threaded applications

## Limitations

- **Performance Overhead**: Recording every span entry/exit adds overhead
- **Memory Usage**: Profiles are kept in memory until exported
- **Thread ID Mapping**: Thread IDs are hashed to u32 for compatibility

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT OR Apache-2.0 license.

## Related Projects

- [tracing](https://github.com/tokio-rs/tracing) - Application-level tracing for Rust
- [fxprof-processed-profile](https://github.com/mstange/samply) - Firefox Profiler format library
- [Firefox Profiler](https://profiler.firefox.com/) - Web-based profiler interface 