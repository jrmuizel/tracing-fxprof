use tracing_fxprof::FxProfSubscriber;
use tracing_subscriber::prelude::*;
use std::thread;
use std::time::Duration;

#[tracing::instrument]
fn fibonacci(n: u64) -> u64 {
    if n <= 1 {
        tracing::debug!("Base case: n = {}", n);
        n
    } else {
        tracing::debug!("Calculating fibonacci({})", n);
        let a = fibonacci(n - 1);
        let b = fibonacci(n - 2);
        let result = a + b;
        tracing::info!("fibonacci({}) = {}", n, result);
        result
    }
}

#[tracing::instrument]
fn simulate_work(task_name: &str, duration_ms: u64) {
    tracing::info!("Starting task: {}", task_name);
    
    let _processing_span = tracing::debug_span!("processing").entered();
    thread::sleep(Duration::from_millis(duration_ms));
    tracing::debug!("Task {} completed in {}ms", task_name, duration_ms);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create the FxProf subscriber
    let subscriber = FxProfSubscriber::new("fxprof-tracing-example");
    
    // Initialize tracing with our subscriber
    let _guard = tracing_subscriber::registry()
        .with(subscriber.clone())
        .set_default();
    
    // Application entry point
    let _main_span = tracing::info_span!("main").entered();
    tracing::info!("Application started");
    
    // Example 1: Mathematical computation with recursive spans
    {
        let _computation_span = tracing::info_span!("computation").entered();
        tracing::info!("Computing fibonacci sequence");
        
        let result = fibonacci(8);
        tracing::info!("Final result: {}", result);
    }
    
    // Example 2: Simulated async work
    {
        let _async_work_span = tracing::info_span!("async_work").entered();
        tracing::info!("Starting parallel tasks");
        
        let handles: Vec<_> = (0..3)
            .map(|i| {
                let task_name = format!("task_{}", i);
                thread::spawn(move || {
                    simulate_work(&task_name, (i + 1) * 100);
                })
            })
            .collect();
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        tracing::info!("All tasks completed");
    }
    
    // Example 3: Error handling
    {
        let _error_handling_span = tracing::warn_span!("error_handling").entered();
        tracing::warn!("Demonstrating error logging");
        
        match std::fs::read("nonexistent_file.txt") {
            Ok(_) => tracing::info!("File read successfully"),
            Err(e) => tracing::error!("Failed to read file: {}", e),
        }
    }
    
    tracing::info!("Application finished");
    
    // Export the profile to a JSON file
    let output_file = "profile.json";
    subscriber.save_profile(output_file)?;
    println!("Profile saved to {}", output_file);
    println!("You can view this profile at https://profiler.firefox.com by dragging and dropping the file");
    
    Ok(())
} 