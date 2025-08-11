use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use fxprof_processed_profile::{
    CategoryHandle, CpuDelta, FrameFlags, Profile, ProcessHandle, 
    SamplingInterval, StackHandle, ThreadHandle, Timestamp, ReferenceTimestamp,
    StringHandle
};
use tracing::{
    span::{Attributes, Id, Record},
    Event, Subscriber,
};
use tracing_subscriber::layer::{Context, Layer};

/// A tracing subscriber that outputs profiles in the Firefox Profiler format
/// 
/// # Example
/// 
/// ```rust
/// use tracing_fxprof::FxProfSubscriber;
/// use tracing_subscriber::prelude::*;
/// 
/// let subscriber = FxProfSubscriber::new("my-app");
/// 
/// // Set up the tracing subscriber
/// let _guard = tracing_subscriber::registry()
///     .with(subscriber.clone())
///     .set_default();
/// 
/// // Your application code with tracing
/// {
///     let _span = tracing::info_span!("main_function").entered();
///     tracing::info!("Starting application");
///     
///     {
///         let _span = tracing::debug_span!("inner_function").entered();
///         tracing::debug!("Processing data");
///     }
/// }
/// 
/// // Export the profile to a file
/// subscriber.save_profile("profile.json").unwrap();
/// ```
#[derive(Clone)]
pub struct FxProfSubscriber {
    inner: Arc<Mutex<FxProfSubscriberInner>>,
}

struct FxProfSubscriberInner {
    profile: Profile,
    process: ProcessHandle,
    threads: HashMap<std::thread::ThreadId, ThreadHandle>,
    spans: HashMap<Id, SpanData>,
    stacks: HashMap<std::thread::ThreadId, Vec<StackHandle>>,
}

struct SpanData {
    name: String,
    target: String,
    stack_handle: Option<StackHandle>,
    thread_id: std::thread::ThreadId,
}

impl FxProfSubscriber {
    /// Create a new FxProfSubscriber
    /// 
    /// # Arguments
    /// 
    /// * `product_name` - The name of the application being profiled
    pub fn new(product_name: &str) -> Self {
        let reference_timestamp = ReferenceTimestamp::from_system_time(SystemTime::now());
        let sampling_interval = SamplingInterval::from_nanos(1_000_000); // 1ms
        
        let mut profile = Profile::new(product_name, reference_timestamp, sampling_interval);
        
        // Set OS name
        #[cfg(target_os = "macos")]
        profile.set_os_name("macOS");
        #[cfg(target_os = "linux")]
        profile.set_os_name("Linux");
        #[cfg(target_os = "windows")]
        profile.set_os_name("Windows");
        
        // Add a default process
        let process = profile.add_process(
            product_name,
            std::process::id(),
            Timestamp::from_nanos_since_reference(0),
        );
        
        Self {
            inner: Arc::new(Mutex::new(FxProfSubscriberInner {
                profile,
                process,
                threads: HashMap::new(),
                spans: HashMap::new(),
                stacks: HashMap::new(),
            })),
        }
    }
    
    /// Export the profile as JSON string
    /// 
    /// This returns the profile data in Firefox Profiler's processed profile format
    /// as a JSON string that can be loaded into the Firefox Profiler web interface.
    pub fn export_profile(&self) -> Result<String, serde_json::Error> {
        let inner = self.inner.lock().unwrap();
        serde_json::to_string_pretty(&inner.profile)
    }
    
    /// Save the profile to a file
    /// 
    /// # Arguments
    /// 
    /// * `path` - The file path where the profile should be saved
    /// 
    /// # Example
    /// 
    /// ```rust,no_run
    /// # use tracing_fxprof::FxProfSubscriber;
    /// let subscriber = FxProfSubscriber::new("my-app");
    /// // ... do some tracing ...
    /// subscriber.save_profile("my-profile.json").unwrap();
    /// ```
    pub fn save_profile<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let json = self.export_profile()?;
        std::fs::write(path, json)?;
        Ok(())
    }
    
    fn get_or_create_thread(inner: &mut FxProfSubscriberInner, thread_id: std::thread::ThreadId) -> ThreadHandle {
        if let Some(&thread) = inner.threads.get(&thread_id) {
            return thread;
        }
        
        // Create a new thread
        let thread_name = std::thread::current().name()
            .unwrap_or("unnamed")
            .to_string();
        
        let thread = inner.profile.add_thread(
            inner.process,
            thread_id_to_u32(thread_id),
            Timestamp::from_nanos_since_reference(0),
            thread_name == "main",
        );
        
        inner.profile.set_thread_name(thread, &thread_name);
        inner.threads.insert(thread_id, thread);
        inner.stacks.insert(thread_id, Vec::new());
        
        thread
    }
    
    fn system_time_to_timestamp(time: SystemTime) -> Timestamp {
        let duration_since_epoch = time.duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0));
        
        // Convert to nanoseconds as u64
        let nanos = duration_since_epoch.as_nanos() as u64;
        Timestamp::from_nanos_since_reference(nanos)
    }
}

impl<S> Layer<S> for FxProfSubscriber 
where
    S: Subscriber,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, _ctx: Context<'_, S>) {
        let mut inner = self.inner.lock().unwrap();
        let thread_id = std::thread::current().id();
        let _thread = Self::get_or_create_thread(&mut inner, thread_id);
        
        let metadata = attrs.metadata();
        let span_data = SpanData {
            name: metadata.name().to_string(),
            target: metadata.target().to_string(),
            stack_handle: None,
            thread_id,
        };
        
        inner.spans.insert(id.clone(), span_data);
    }
    
    fn on_enter(&self, id: &Id, _ctx: Context<'_, S>) {
        let mut inner = self.inner.lock().unwrap();
        
        // First get the span data to avoid borrowing conflicts
        let (thread_id, frame_name) = if let Some(span_data) = inner.spans.get(id) {
            let frame_name = format!("{}::{}", span_data.target, span_data.name);
            (span_data.thread_id, frame_name)
        } else {
            return;
        };
        
        let thread = Self::get_or_create_thread(&mut inner, thread_id);
        
        // Create frame info for this span
        let string_handle = inner.profile.handle_for_string(&frame_name);
        
        // Get the current stack
        let current_stacks = inner.stacks.get(&thread_id).cloned().unwrap_or_default();
        let parent_stack = current_stacks.last().copied();
        
        // Create frame and stack
        let frame_handle = inner.profile.handle_for_frame_with_label(
            thread,
            string_handle,
            CategoryHandle::OTHER,
            FrameFlags::empty(),
        );
        let stack_handle = inner.profile.handle_for_stack(thread, frame_handle, parent_stack);
        
        // Update the span with its stack handle
        if let Some(span_data) = inner.spans.get_mut(id) {
            span_data.stack_handle = Some(stack_handle);
        }
        
        // Push the stack onto the thread's stack
        inner.stacks.get_mut(&thread_id).unwrap().push(stack_handle);
        
        // Add a sample for span entry
        let timestamp = Self::system_time_to_timestamp(SystemTime::now());
        inner.profile.add_sample(
            thread,
            timestamp,
            Some(stack_handle),
            CpuDelta::ZERO,
            1,
        );
    }
    
    fn on_exit(&self, id: &Id, _ctx: Context<'_, S>) {
        let mut inner = self.inner.lock().unwrap();
        
        let thread_id = if let Some(span_data) = inner.spans.get(id) {
            span_data.thread_id
        } else {
            return;
        };
        
        let thread = Self::get_or_create_thread(&mut inner, thread_id);
        
        // Pop the stack
        if let Some(stacks) = inner.stacks.get_mut(&thread_id) {
            stacks.pop();
            
            // Add a sample for span exit
            let timestamp = Self::system_time_to_timestamp(SystemTime::now());
            let current_stack = stacks.last().copied();
            
            inner.profile.add_sample(
                thread,
                timestamp,
                current_stack,
                CpuDelta::ZERO,
                1,
            );
        }
    }
    
    fn on_close(&self, id: Id, _ctx: Context<'_, S>) {
        let mut inner = self.inner.lock().unwrap();
        inner.spans.remove(&id);
    }
    
    fn on_record(&self, _id: &Id, _values: &Record<'_>, _ctx: Context<'_, S>) {
        // Could be used to record span fields as markers or metadata
    }
    
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut inner = self.inner.lock().unwrap();
        let thread_id = std::thread::current().id();
        let thread = Self::get_or_create_thread(&mut inner, thread_id);
        
        // Create a marker for the event
        let metadata = event.metadata();
        let event_name = format!("{}::{}", metadata.target(), metadata.name());
        let name_handle = inner.profile.handle_for_string(&event_name);
        
        // Create a simple text marker for the event
        let marker = SimpleTextMarker {
            name: name_handle,
            text: name_handle, // Could extract actual message here
        };
        
        let timing = fxprof_processed_profile::MarkerTiming::Instant(
            Self::system_time_to_timestamp(SystemTime::now())
        );
        
        let marker_handle = inner.profile.add_marker(thread, timing, marker);
        
        // If we're currently in a span, set the marker's stack
        if let Some(stacks) = inner.stacks.get(&thread_id) {
            if let Some(&current_stack) = stacks.last() {
                inner.profile.set_marker_stack(thread, marker_handle, Some(current_stack));
            }
        }
    }
}

// Helper function to convert ThreadId to u32
fn thread_id_to_u32(thread_id: std::thread::ThreadId) -> u32 {
    // This is a simple hash of the thread ID since we can't directly convert it
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    thread_id.hash(&mut hasher);
    hasher.finish() as u32
}

// Simple marker implementation for events
#[derive(Debug, Clone)]
struct SimpleTextMarker {
    name: StringHandle,
    text: StringHandle,
}

impl fxprof_processed_profile::StaticSchemaMarker for SimpleTextMarker {
    const UNIQUE_MARKER_TYPE_NAME: &'static str = "TracingEvent";
    const CHART_LABEL: Option<&'static str> = Some("{marker.data.text}");
    const TABLE_LABEL: Option<&'static str> = Some("{marker.name} - {marker.data.text}");
    
    const FIELDS: &'static [fxprof_processed_profile::StaticSchemaMarkerField] = &[
        fxprof_processed_profile::StaticSchemaMarkerField {
            key: "text",
            label: "Message",
            format: fxprof_processed_profile::MarkerFieldFormat::String,
            flags: fxprof_processed_profile::MarkerFieldFlags::SEARCHABLE,
        }
    ];

    fn name(&self, _profile: &mut Profile) -> StringHandle {
        self.name
    }

    fn string_field_value(&self, _field_index: u32) -> StringHandle {
        self.text
    }

    fn number_field_value(&self, _field_index: u32) -> f64 {
        unreachable!()
    }

    fn flow_field_value(&self, _field_index: u32) -> u64 {
        unreachable!()
    }
}




#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::prelude::*;
    
    #[test]
    fn test_fxprof_subscriber() {
        let subscriber = FxProfSubscriber::new("test-app");
        
        let _guard = tracing_subscriber::registry()
            .with(subscriber.clone())
            .set_default();
        
        // Create some spans and events
        let _span = tracing::info_span!("test_span").entered();
        tracing::info!("test event");
        
        // Export the profile
        let profile_json = subscriber.export_profile().unwrap();
        assert!(profile_json.contains("test-app"));
        assert!(profile_json.contains("test_span"));
    }
}
