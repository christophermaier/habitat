// Copyright (c) 2016-2017 Chef Software Inc. and/or applicable contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{
    num::ParseIntError,
    str::FromStr,
    sync::mpsc::{channel, Receiver},
    thread::Builder,
    time::Duration,
};

use notify::{DebouncedEvent, RecommendedWatcher, RecursiveMode, Watcher};

use super::spec_dir::SpecDir;
use config::EnvConfig;
use error::Result;

/// How long should we wait to consolidate filesystem events?
///
/// This should strike a balance between responsiveness and
/// too-granular a series of events.
///
/// See https://docs.rs/notify/4.0.6/notify/trait.Watcher.html#tymethod.new
struct SpecWatcherDelay(Duration);

impl From<Duration> for SpecWatcherDelay {
    fn from(d: Duration) -> SpecWatcherDelay {
        SpecWatcherDelay(d)
    }
}

impl Default for SpecWatcherDelay {
    fn default() -> Self {
        SpecWatcherDelay(Duration::from_millis(2_000))
    }
}

impl FromStr for SpecWatcherDelay {
    type Err = ParseIntError;
    fn from_str(s: &str) -> ::std::result::Result<Self, Self::Err> {
        // u16 ~= 65 seconds, which is _more_ than enough
        let raw = s.parse::<u16>()?;
        Ok(Duration::from_millis(raw as u64).into())
    }
}

impl EnvConfig for SpecWatcherDelay {
    const ENVVAR: &'static str = "HAB_SPEC_WATCHER_DELAY_MS";
}

// TODO (CM): Instead of having a separate SpecWatcher, what if we
// folded this into SpecDir itself?

// TODO (CM): Implement Debug?
pub struct SpecWatcher {
    // Not actually used; only holding onto it for lifetime / Drop purposes.
    _watcher: RecommendedWatcher,
    channel: Receiver<DebouncedEvent>,
}

impl SpecWatcher {
    /// Start up a separate thread to listen for filesystem
    /// events.
    pub fn run(spec_dir: SpecDir) -> Result<SpecWatcher> {
        // The act of creating a `notify::Watcher` creates threads on
        // its own. It does not, however, allow you to set the _names_
        // of those threads.
        //
        // We're creating a SpecWatcher in a thread just so we can get
        // some control over the name of the threads that the
        // underlying `notify::Watcher` creates, which makes
        // monitoring and reasoning about the overall Supervisor
        // process easier. There's no other reason than that; if the
        // `notify` crate allowed us to name the threads, we could
        // simplify this.

        // TODO (CM): remove all the expects in this
        let (tx, rx) = channel();
        Builder::new()
            .name(String::from("spec-watcher"))
            .spawn(move || {
                let (event_tx, event_rx) = channel();
                let delay = SpecWatcherDelay::configured_value();
                let mut watcher = RecommendedWatcher::new(event_tx, delay.0).expect("lol");
                watcher
                    .watch(spec_dir, RecursiveMode::NonRecursive)
                    .expect("lulz");
                let sw = SpecWatcher {
                    _watcher: watcher,
                    channel: event_rx,
                };
                tx.send(sw).expect("Could not send SpecWatcher");
            })?.join()
            .expect("whee");

        Ok(rx.recv()?)
        // There's no way this should take 10 seconds
        //Ok(rx.recv_timeout(Duration::from_secs(10))?)
    }

    /// Returns `true` if any filesystem events were detected in the
    /// watched directory.
    pub fn has_events(&self) -> bool {
        trace!("Asking for spec events");
        // TODO (CM): Could filter the events for those that only
        // impact spec files
        !self.channel.try_iter().collect::<Vec<_>>().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs::File,
        io::{Error as IoError, Write},
        result::Result as StdResult,
        thread,
    };
    use tempfile::TempDir;

    locked_env_var!(HAB_SPEC_WATCHER_DELAY_MS, lock_delay_var);

    fn file_with_content<C>(dir: &TempDir, filename: &str, contents: C) -> StdResult<(), IoError>
    where
        C: Into<String>,
    {
        let path = dir.path().join(filename);
        let mut buffer = File::create(&path)?;
        buffer.write_all(contents.into().as_bytes())
    }

    /// Sleep for the currently-configured debounce interval, plus a
    /// few milliseconds more, just to be certain our filesystem
    /// events have had plenty of time to process.
    fn wait_for_debounce_interval() {
        thread::sleep(SpecWatcherDelay::configured_value().0 + Duration::from_millis(2));
    }

    #[test]
    fn can_be_created() {
        let _delay = lock_delay_var();

        let dir = TempDir::new().expect("Could not create directory");
        let spec_dir = SpecDir::new(dir.path()).expect("Couldn't make SpecDir");
        assert!(
            SpecWatcher::run(spec_dir).is_ok(),
            "Couldn't create a SpecWatcher!"
        );
    }

    #[test]
    fn can_get_events_for_spec_files() {
        let _delay = lock_delay_var();

        let dir = TempDir::new().expect("Could not create directory");
        let spec_dir = SpecDir::new(dir.path()).expect("Couldn't make SpecDir");
        let sw = SpecWatcher::run(spec_dir).expect("Couldn't create a SpecWatcher!");

        assert!(!sw.has_events(), "There should be no events to start");

        file_with_content(&dir, "foo.spec", "fooooooo").expect("couldn't create file");

        assert!(
            !sw.has_events(),
            "Need to allow for the debounce interval to pass before you can expect events"
        );

        wait_for_debounce_interval();

        assert!(sw.has_events(), "There should be an event now");
        assert!(
            !sw.has_events(),
            "Should be no more events after you've checked"
        );
    }

    /// Currently, the spec watcher will respond to changes to any
    /// file in the directory, whether it's a `*.spec` file or not.
    ///
    /// This would, for instance, pick up the temp files that
    /// operations like `hab svc stop` lay down before renaming them
    /// to their final `*.spec` form.
    #[test]
    fn can_get_events_for_non_spec_files() {
        let _delay = lock_delay_var();

        let dir = TempDir::new().expect("Could not create directory");
        let spec_dir = SpecDir::new(dir.path()).expect("Couldn't make SpecDir");
        let sw = SpecWatcher::run(spec_dir).expect("Couldn't create a SpecWatcher!");

        assert!(!sw.has_events(), "There should be no events to start");

        file_with_content(&dir, "foo.abc123xyz", "fooooooo").expect("couldn't create file");

        assert!(
            !sw.has_events(),
            "Need to allow for the debounce interval to pass before you can expect events"
        );

        wait_for_debounce_interval();

        assert!(sw.has_events(), "There should be an event now");
        assert!(
            !sw.has_events(),
            "Should be no more events after you've checked"
        );
    }

    #[test]
    fn short_debounce_delays_also_work() {
        let delay = lock_delay_var();
        delay.set("1");

        // Just verifying that our delay variable works correctly
        assert_eq!(
            SpecWatcherDelay::configured_value().0,
            Duration::from_millis(1)
        );

        let dir = TempDir::new().expect("Could not create directory");
        let spec_dir = SpecDir::new(dir.path()).expect("Couldn't make SpecDir");
        let sw = SpecWatcher::run(spec_dir).expect("Couldn't create a SpecWatcher!");

        assert!(!sw.has_events(), "There should be no events to start");

        file_with_content(&dir, "foo.spec", "fooooooo").expect("couldn't create file");

        assert!(
            !sw.has_events(),
            "Need to allow for the debounce interval to pass before you can expect events"
        );

        wait_for_debounce_interval();

        assert!(sw.has_events(), "There should be an event now");
        assert!(
            !sw.has_events(),
            "Should be no more events after you've checked"
        );
    }

}
// #[cfg(test)]
// mod test {
//     use std::collections::HashMap;
//     use std::fs;
//     use std::io::Write;
//     use std::path::Path;
//     use std::str::FromStr;
//     use std::sync::mpsc::Sender;
//     use std::thread;
//     use std::time::{Duration, Instant};

//     use hcore::package::PackageIdent;
//     use notify;
//     use tempfile::TempDir;

//     use super::{MySpecWatcher, SpecWatcher, SpecWatcherEvent};
//     use error::Error::*;
//     use manager::service::ServiceSpec;

//     #[test]
//     fn run_watch_dir_not_created() {
//         let tmpdir = TempDir::new().unwrap();
//         let not_a_dir = tmpdir.path().join("i-dont-exist");

//         match SpecWatcher::run(&not_a_dir) {
//             Err(e) => match e.err {
//                 SpecWatcherDirNotFound(dir) => assert_eq!(dir, not_a_dir.display().to_string()),
//                 wrong => panic!("Unexpected error returned: {:?}", wrong),
//             },
//             Ok(_) => panic!("Watcher should fail to run"),
//         }
//     }

//     #[test]
//     fn run_with_notify_error() {
//         let tmpdir = TempDir::new().unwrap();
//         let path = tmpdir.path().join("throw_error");
//         fs::create_dir(&path).unwrap();

//         match SpecWatcher::run_with::<TestWatcher, _>(&path) {
//             Ok(_) => assert!(true),
//             Err(e) => panic!("This should not fail: {:?}", e.err),
//         }
//     }

//     #[test]
//     fn new_events_no_change_with_no_active_specs() {
//         let tmpdir = TempDir::new().unwrap();
//         let path = tmpdir.path().join("no_events");
//         fs::create_dir(&path).unwrap();

//         let active_specs = map_for_specs(vec![]);
//         let mut watcher = SpecWatcher::run_with::<TestWatcher, _>(&path).unwrap();
//         let events = watcher.new_events(active_specs).unwrap();

//         assert_eq!(events, vec![]);
//     }

//     #[test]
//     fn new_events_no_change_with_active_specs() {
//         let tmpdir = TempDir::new().unwrap();
//         let path = tmpdir.path().join("no_events");
//         fs::create_dir(&path).unwrap();
//         new_saved_spec(&path, "acme/alpha");
//         new_saved_spec(&path, "acme/beta");

//         let active_specs = map_for_specs(vec!["acme/alpha", "acme/beta"]);
//         let mut watcher = SpecWatcher::run_with::<TestWatcher, _>(&path).unwrap();
//         let events = watcher.new_events(active_specs).unwrap();

//         assert_eq!(events, vec![]);
//     }

//     #[test]
//     fn new_events_new_spec_with_no_active_specs() {
//         let tmpdir = TempDir::new().unwrap();
//         let path = tmpdir.path().join("new_spec");
//         fs::create_dir(&path).unwrap();
//         let newbie = new_spec("acme/newbie");

//         let active_specs = map_for_specs(vec![]);
//         let mut watcher = SpecWatcher::run_with::<TestWatcher, _>(&path).unwrap();
//         let events = waiting_for_new_events(&mut watcher, active_specs);

//         assert_eq!(1, events.len());
//         assert!(events.contains(&SpecWatcherEvent::AddService(newbie)));
//     }

//     #[test]
//     fn new_events_new_spec_with_active_specs() {
//         let tmpdir = TempDir::new().unwrap();
//         let path = tmpdir.path().join("new_spec");
//         fs::create_dir(&path).unwrap();
//         new_saved_spec(&path, "acme/alpha");
//         new_saved_spec(&path, "acme/beta");
//         let newbie = new_spec("acme/newbie");

//         let active_specs = map_for_specs(vec!["acme/alpha", "acme/beta"]);
//         let mut watcher = SpecWatcher::run_with::<TestWatcher, _>(&path).unwrap();
//         let events = waiting_for_new_events(&mut watcher, active_specs);

//         assert_eq!(1, events.len());
//         assert!(events.contains(&SpecWatcherEvent::AddService(newbie)));
//     }

//     #[test]
//     fn new_events_removed_spec_with_active_specs() {
//         let tmpdir = TempDir::new().unwrap();
//         let path = tmpdir.path().join("removed_spec");
//         fs::create_dir(&path).unwrap();
//         new_saved_spec(&path, "acme/alpha");
//         new_saved_spec(&path, "acme/beta");
//         let oldie = new_saved_spec(&path, "acme/oldie");

//         let active_specs = map_for_specs(vec!["acme/alpha", "acme/beta", "acme/oldie"]);
//         let mut watcher = SpecWatcher::run_with::<TestWatcher, _>(&path).unwrap();
//         let events = waiting_for_new_events(&mut watcher, active_specs);

//         assert_eq!(1, events.len());
//         assert!(events.contains(&SpecWatcherEvent::RemoveService(oldie)));
//     }

//     #[test]
//     fn new_events_add_and_removed_spec_with_active_specs() {
//         let tmpdir = TempDir::new().unwrap();
//         let path = tmpdir.path().join("new_and_removed_spec");
//         fs::create_dir(&path).unwrap();
//         new_saved_spec(&path, "acme/alpha");
//         new_saved_spec(&path, "acme/beta");
//         let oldie = new_saved_spec(&path, "acme/oldie");
//         let newbie = new_spec("acme/newbie");

//         let active_specs = map_for_specs(vec!["acme/alpha", "acme/beta", "acme/oldie"]);
//         let mut watcher = SpecWatcher::run_with::<TestWatcher, _>(&path).unwrap();
//         let events = waiting_for_new_events(&mut watcher, active_specs);

//         assert_eq!(2, events.len());
//         assert!(events.contains(&SpecWatcherEvent::RemoveService(oldie)));
//         assert!(events.contains(&SpecWatcherEvent::AddService(newbie)));
//     }

//     #[test]
//     fn new_events_changed_spec_with_active_specs() {
//         let tmpdir = TempDir::new().unwrap();
//         let path = tmpdir.path().join("changed_spec");
//         fs::create_dir(&path).unwrap();
//         new_saved_spec(&path, "acme/alpha");
//         new_saved_spec(&path, "acme/beta");
//         let transformer_before = new_saved_spec(&path, "acme/transformer");
//         let mut transformer_after = new_spec("acme/transformer");
//         transformer_after.group = String::from("autobots");

//         let active_specs = map_for_specs(vec!["acme/alpha", "acme/beta", "acme/transformer"]);
//         let mut watcher = SpecWatcher::run_with::<TestWatcher, _>(&path).unwrap();
//         let events = waiting_for_new_events(&mut watcher, active_specs);

//         assert_eq!(2, events.len());
//         assert_eq!(
//             events[0],
//             SpecWatcherEvent::RemoveService(transformer_before)
//         );
//         assert_eq!(events[1], SpecWatcherEvent::AddService(transformer_after));
//     }

//     #[test]
//     fn new_events_crazytown_with_active_specs() {
//         let tmpdir = TempDir::new().unwrap();
//         let path = tmpdir.path().join("crazytown");
//         fs::create_dir(&path).unwrap();
//         new_saved_spec(&path, "acme/alpha");
//         new_saved_spec(&path, "acme/beta");
//         let oldie = new_saved_spec(&path, "acme/oldie");
//         let newbie = new_spec("acme/newbie");
//         let transformer_before = new_saved_spec(&path, "acme/transformer");
//         let mut transformer_after = new_spec("acme/transformer");
//         transformer_after.group = String::from("autobots");

//         let active_specs = map_for_specs(vec![
//             "acme/alpha",
//             "acme/beta",
//             "acme/oldie",
//             "acme/transformer",
//         ]);
//         let mut watcher = SpecWatcher::run_with::<TestWatcher, _>(&path).unwrap();
//         let events = waiting_for_new_events(&mut watcher, active_specs);

//         assert_eq!(4, events.len());
//         assert!(events.contains(&SpecWatcherEvent::RemoveService(oldie)));
//         assert!(events.contains(&SpecWatcherEvent::AddService(newbie)));
//         assert!(events.contains(&SpecWatcherEvent::RemoveService(transformer_before),));
//         assert!(events.contains(&SpecWatcherEvent::AddService(transformer_after),));
//     }

//     // #[test]
//     // fn loading_spec_missing_ident_doesnt_impact_others() {
//     //     let tmpdir = TempDir::new().unwrap();
//     //     let alpha = new_saved_spec(tmpdir.path(), "acme/alpha");
//     //     fs::File::create(tmpdir.path().join(format!("beta.spec"))).expect("can't create file");

//     //     let mut watcher = SpecWatcher::run(tmpdir.path()).unwrap();

//     //     let events = watcher.initial_events().unwrap();

//     //     assert_eq!(1, events.len());
//     //     assert!(events.contains(&SpecWatcherEvent::AddService(alpha)));
//     // }

//     // #[test]
//     // fn loading_spec_bad_content_doesnt_impact_others() {
//     //     let tmpdir = TempDir::new().unwrap();
//     //     let alpha = new_saved_spec(tmpdir.path(), "acme/alpha");
//     //     {
//     //         let mut bad = fs::File::create(tmpdir.path().join(format!("beta.spec")))
//     //             .expect("can't create file");
//     //         bad.write_all(
//     //             r#"ident = "acme/beta"
//     //                       I am a bad bad file."#
//     //                 .as_bytes(),
//     //         ).expect("can't write file content");
//     //     }

//     //     let mut watcher = SpecWatcher::run(tmpdir.path()).unwrap();

//     //     let events = watcher.initial_events().unwrap();

//     //     assert_eq!(1, events.len());
//     //     assert!(events.contains(&SpecWatcherEvent::AddService(alpha)));
//     // }

//     // #[test]
//     // fn loading_spec_ident_name_mismatch_doesnt_impact_others() {
//     //     let tmpdir = TempDir::new().unwrap();
//     //     let alpha = new_saved_spec(tmpdir.path(), "acme/alpha");
//     //     {
//     //         let mut bad = fs::File::create(tmpdir.path().join(format!("beta.spec")))
//     //             .expect("can't create file");
//     //         bad.write_all(r#"ident = "acme/NEAL_MORSE_BAND""#.as_bytes())
//     //             .expect("can't write file content");
//     //     }

//     //     let mut watcher = SpecWatcher::run(tmpdir.path()).unwrap();

//     //     let events = watcher.initial_events().unwrap();

//     //     assert_eq!(1, events.len());
//     //     assert!(events.contains(&SpecWatcherEvent::AddService(alpha)));
//     // }

//     struct TestWatcher {
//         tx: Sender<notify::DebouncedEvent>,
//     }

//     impl TestWatcher {
//         fn behavior_new_spec<P: AsRef<Path>>(&mut self, path: P) {
//             new_saved_spec(path.as_ref(), "acme/newbie");
//             self.tx
//                 .send(notify::DebouncedEvent::Write(
//                     path.as_ref().join("newbie.spec"),
//                 )).expect("couldn't send event");
//         }

//         fn behavior_removed_spec<P: AsRef<Path>>(&mut self, path: P) {
//             let toml_path = path.as_ref().join("oldie.spec");
//             fs::remove_file(&toml_path).expect("couldn't delete spec toml");
//             self.tx
//                 .send(notify::DebouncedEvent::Remove(toml_path))
//                 .expect("couldn't send event");
//         }

//         fn behavior_changed_spec<P: AsRef<Path>>(&mut self, path: P) {
//             let toml_path = path.as_ref().join("transformer.spec");
//             let mut spec = ServiceSpec::from_file(&toml_path).expect("couldn't load spec file");
//             spec.group = String::from("autobots");
//             spec.to_file(&toml_path).expect("couldn't write spec file");
//             self.tx
//                 .send(notify::DebouncedEvent::Write(toml_path))
//                 .expect("couldn't send event");
//         }
//     }

//     impl notify::Watcher for TestWatcher {
//         fn new(tx: Sender<notify::DebouncedEvent>, _delay: Duration) -> notify::Result<Self> {
//             Ok(TestWatcher { tx: tx })
//         }

//         fn watch<P: AsRef<Path>>(
//             &mut self,
//             path: P,
//             _recursive_mode: notify::RecursiveMode,
//         ) -> notify::Result<()> {
//             let behavior = path
//                 .as_ref()
//                 .file_name()
//                 .expect("file name is ..")
//                 .to_str()
//                 .expect("path isn't utf-8 valid");

//             match behavior {
//                 "no_events" => {}
//                 "new_spec" => self.behavior_new_spec(path.as_ref()),
//                 "removed_spec" => self.behavior_removed_spec(path.as_ref()),
//                 "new_and_removed_spec" => {
//                     self.behavior_new_spec(path.as_ref());
//                     self.behavior_removed_spec(path.as_ref());
//                 }
//                 "changed_spec" => self.behavior_changed_spec(path.as_ref()),
//                 "crazytown" => {
//                     self.behavior_changed_spec(path.as_ref());
//                     self.behavior_new_spec(path.as_ref());
//                     self.behavior_removed_spec(path.as_ref());
//                 }
//                 "throw_error" => {
//                     return Err(notify::Error::Generic(String::from("we failed you, noes!")));
//                 }
//                 unknown => panic!("unknown fixture behavior: {}", unknown),
//             }

//             Ok(())
//         }

//         fn new_raw(_tx: Sender<notify::RawEvent>) -> notify::Result<Self> {
//             unimplemented!()
//         }

//         fn unwatch<P: AsRef<Path>>(&mut self, _path: P) -> notify::Result<()> {
//             unimplemented!()
//         }
//     }

//     fn new_spec(ident: &str) -> ServiceSpec {
//         ServiceSpec::default_for(PackageIdent::from_str(ident).expect("couldn't parse ident str"))
//     }

//     fn new_saved_spec(tmpdir: &Path, ident: &str) -> ServiceSpec {
//         let spec = new_spec(ident);
//         spec.to_file(tmpdir.join(format!("{}.spec", &spec.ident.name)))
//             .expect("couldn't save spec to disk");
//         spec
//     }

//     fn map_for_specs(idents: Vec<&str>) -> HashMap<String, ServiceSpec> {
//         let mut map = HashMap::new();
//         for ident in idents {
//             let spec = ServiceSpec::default_for(
//                 PackageIdent::from_str(ident).expect("couldn't parse ident str"),
//             );
//             map.insert(spec.ident.name.clone(), spec);
//         }
//         map
//     }

//     fn waiting_for_new_events(
//         watcher: &mut SpecWatcher,
//         active_specs: HashMap<String, ServiceSpec>,
//     ) -> Vec<SpecWatcherEvent> {
//         let start = Instant::now();
//         let timeout = Duration::from_millis(1000);
//         while start.elapsed() < timeout {
//             let events = watcher.new_events(active_specs.clone()).unwrap();
//             if !events.is_empty() {
//                 return events;
//             }
//             thread::sleep(Duration::from_millis(1));
//         }
//         panic!("Waited for events but found none");
//     }
// }
