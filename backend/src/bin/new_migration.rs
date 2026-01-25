//! Cargo command to generate a new migration file with a unique timestamp
//! 
//! Usage: cargo run --bin new_migration -- <description>
//! Example: cargo run --bin new_migration -- add_user_preferences

use std::fs;
use std::path::{Path, PathBuf};
use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Error: Migration description is required");
        eprintln!("Usage: cargo run --bin new_migration -- <description>");
        eprintln!("Example: cargo run --bin new_migration -- add_user_preferences");
        process::exit(1);
    }
    
    let description = &args[1];
    let description = sanitize_description(description);
    
    let migrations_dir = PathBuf::from("migrations");
    if !migrations_dir.exists() {
        eprintln!("Error: migrations directory not found");
        process::exit(1);
    }
    
    // Generate timestamp
    let timestamp = generate_unique_timestamp(&migrations_dir);
    
    let migration_file = migrations_dir.join(format!("{}_{}.sql", timestamp, description));
    
    // Create migration file with template
    let template = format!(
        "-- Migration: {}\n\
         -- Created: {}\n\
         \n\
         -- Add your migration SQL here\n\
         \n",
        description,
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    );
    
    fs::write(&migration_file, template)
        .expect("Failed to write migration file");
    
    println!("Created migration file: {}", migration_file.display());
    println!();
    println!("Next steps:");
    println!("1. Edit the migration file to add your SQL");
    println!("2. Test the migration: cargo test");
    println!("3. Apply the migration: The migration will run automatically on next app start");
}

fn sanitize_description(description: &str) -> String {
    description
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .replace("__", "_")
}

fn generate_unique_timestamp(migrations_dir: &Path) -> String {
    let mut timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S").to_string();
    
    // Check if a migration with this timestamp already exists
    loop {
        let mut found = false;
        
        if let Ok(entries) = fs::read_dir(migrations_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with(&timestamp) && name.ends_with(".sql") {
                        found = true;
                        break;
                    }
                }
            }
        }
        
        if !found {
            break;
        }
        
        // Wait a bit and try again
        std::thread::sleep(std::time::Duration::from_millis(100));
        timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S").to_string();
    }
    
    timestamp
}
