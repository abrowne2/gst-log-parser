// Copyright (C) 2017-2019 Guillaume Desmottes <guillaume@desmottes.be>
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Generate input logs with: GST_DEBUG="GST_TRACER:7" GST_TRACERS=latency\(flags="pipeline+element+reported"\)

use failure::Error;
use gst_log_parser::parse;
use gstreamer::{ClockTime, DebugLevel};
use itertools::Itertools;
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use structopt::StructOpt;

use regex::Regex;

#[derive(StructOpt, Debug)]
#[structopt(name = "latency")]
struct Opt {
    #[structopt(parse(from_os_str))]
    input: PathBuf,
    #[structopt(
        name = "sort-element",
        long = "sort-element",
        about = "Boolean that decides how the output is organized; by element or by src."
    )]
    sort_by_element: bool,
    #[structopt(
        name = "element-filter",
        long = "element-filter",
        about = "Filter that decides which elements to include in the output."
    )]
    element_filter: Option<String>,
    #[structopt(
        name = "blanket-view",
        long = "blanket-view",
        about = "Lumps all the elements under their same `type`; if they're the same element will sum all their latencies."
    )]
    total_element_view: bool,   
}

#[derive(Debug)]
struct Count {
    n: u64,
    total: ClockTime,
}

impl Count {
    fn new() -> Self {
        Self {
            n: 0,
            total: ClockTime::from_nseconds(0),
        }
    }

    fn mean(&self) -> ClockTime {
        ClockTime::from_nseconds(self.total.nseconds() / self.n)
    }
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct Element {
    name: String,
}

impl Element {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}
#[derive(Debug)]
struct Latency {
    element_totals: HashMap<Element, Count>,
}

impl Latency {
    fn new() -> Self {
        Self {
            element_totals: HashMap::new(),
        }
    }

    fn insert_or_get_count(&self, element: &Element) -> &mut Count {
        let count: &mut Count = self.element_totals
            .entry(*element)
            .or_insert_with(Count::new);

        count
    }
    
    fn update_count(&self, element: Element, new_time: u64) {
        let mut count = self.element_totals.get_mut(&element).unwrap();
        count.n += 1;
        count.total += ClockTime::from_nseconds(new_time);
    }
    
    /* Replaces `"gesvideourisource0-videoconvertscale"` as "gesvideourisource-videoconvertscale"
    And `"gesvideourisource0"` as "gesvideourisource" */
    fn normalize_name(&self, name: &str) -> String {
        let parts: Vec<&str> = name.split('-').collect();
        let re = Regex::new(r"\d+$").unwrap(); 

        let first_part = re.replace(parts[0], ""); 

        if parts.len() > 1 {
            let remaining_parts: Vec<&str> = parts.into_iter().skip(1).collect();
            format!("{}-{}", first_part, remaining_parts.join("-")) 
        } else {
            first_part.to_string()
        }
    }
}

fn main() -> Result<(), Error> {
    let opt = Opt::from_args();
    let input = File::open(opt.input)?;
    let latency = Latency::new();
    let element_filter: Option<Regex> = opt.element_filter.map(|f| Regex::new(&f).unwrap());

    let mut elt_latency: HashMap<String, Count> = HashMap::new();
    let parsed = parse(input)
        .filter(|entry| entry.category == "GST_TRACER" && entry.level == DebugLevel::Trace);

    for entry in parsed {
        let s = match entry.message_to_struct() {
            None => continue,
            Some(s) => s,
        };
        match s.name().as_str() {
            "element-latency" => {
                let entry_key = if opt.sort_by_element {
                    s.get::<String>("element").expect("Missing 'element' field")
                } else {
                    s.get::<String>("src").expect("Missing 'src' field")
                };

                let entry_key = if opt.total_element_view {
                    latency.normalize_name(&entry_key)
                } else {
                    entry_key
                };

                if let Some(element_filter) = element_filter.as_ref() {
                    if !element_filter.is_match(&entry_key) {
                        continue;
                    }
                }

                let element = Element::new(&entry_key);
                let count = latency.insert_or_get_count(&element);

                let time: u64 = s.get("time").expect("Missing 'time' field");
                latency.update_count(element, time);
            }
            "latency" => { /* TODO */ }
            "element-reported-latency" => { /* TODO */ }
            _ => {}
        };
    }

    println!("Mean latency:");
    // Sort by pad name so we can easily compare results
    for (pad, count) in latency.element_totals.iter().sorted_by(|(a, _), (b, _)| a.name.cmp(&b.name)) {
        println!("  {}: {}", pad.name, count.mean());
    }

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_name_valid() {
        let latency = Latency::new();
        let mut sample_element_name_hyphenated = "gesvideourisource0-videoconvertscale";
        let mut sample_element_regular = "queue0";
        let mut sample_element_increment = "multiqueue184";
        let mut sample_element_nvh264enc = "nvh264enc";

        assert_eq!(latency.normalize_name(sample_element_name_hyphenated), "gesvideourisource-videoconvertscale");
        assert_eq!(latency.normalize_name(sample_element_increment), "multiqueue");
        assert_eq!(latency.normalize_name(sample_element_regular), "queue");
        assert_eq!(latency.normalize_name(sample_element_nvh264enc), "nvh264enc");
    }
}