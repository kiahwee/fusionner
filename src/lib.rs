extern crate git2;
extern crate libgit2_sys as git2_raw;
#[macro_use]
extern crate log;
extern crate regex;
extern crate rustc_serialize;
extern crate toml;
#[cfg(test)]
extern crate tempdir;
#[cfg(test)]
extern crate url;

#[macro_use]
mod utils;
#[cfg(test)]
#[macro_use]
mod test;
pub mod merger;
pub mod git;

use std::collections::HashSet;
use std::vec::Vec;

use regex::RegexSet;

#[derive(RustcDecodable, RustcEncodable, Eq, PartialEq, Clone, Debug)]
/// Configuration struct for the repository
pub struct RepositoryConfiguration {
    /// URI to the repository remote.
    pub uri: String,
    /// Path to checkout the repository to locally
    pub checkout_path: String,
    /// Name of the remote to use. Defaults to `origin`
    pub remote: Option<String>,
    /// Metadata generated by fusionner is stored as Git notes. This is used to specify the
    /// namespace to store the note in. Defaults to `fusionner`
    pub notes_namespace: Option<String>,
    /// Fetch refspecs to add to the repository being checked out.
    /// You should make sure that the references you are watching is covered by these refspecs
    pub fetch_refspecs: Vec<String>,
    /// Push refspecs to add to the repository being checked out.
    /// This is not really useful at the moment because fusionner will always merge to
    /// refs/fusionner/* and automatically adds the right refspecs for you
    pub push_refspecs: Vec<String>,
    // Authentication Options
    /// Username to authenticate with the remote
    pub username: Option<String>,
    /// Password to authenticate with the remote
    pub password: Option<String>,
    /// Path to private key to authenticate with the remote. If the remote requrests for a key and
    /// this is not specified, we will try to request the key from ssh-agent
    pub key: Option<String>,
    /// Passphrase to the private key for authentication
    pub key_passphrase: Option<String>,
    /// The reference to the "default" branch to create merge commits against
    /// This defaults to the rempte's HEAD, usually refs/heads/master
    pub target_ref: Option<String>, // TODO: Support specifying branch name instead of references
    /// The name to create merge commits under.
    /// If unspecified, will use the global configuration in Git. Otherwise we will use some generic one
    pub signature_name: Option<String>,
    /// The email to create merge commits under.
    /// If unspecified, will use the global configuration in Git. Otherwise we will use some generic one
    pub signature_email: Option<String>,
}

/// "Compiled" watch reference
#[derive(Debug)]
pub struct WatchReferences {
    regex_set: RegexSet,
    exact_list: Vec<String>,
}

impl RepositoryConfiguration {
    pub fn resolve_target_ref(&self, remote: &mut git::Remote) -> Result<String, git2::Error> {
        match self.target_ref {
            Some(ref reference) => {
                info!("Target Reference Specified: {}", reference);
                let remote_refs = remote.remote_ls()?;
                if let None = remote_refs.iter().find(|head| &head.name == reference) {
                    return Err(git_err!(&format!("Could not find {} on remote", reference)));
                }
                Ok(reference.to_string())
            }
            None => {
                let head = remote.head()?;
                if let None = head {
                    return Err(git_err!("Could not find a default HEAD on remote"));
                }
                let head = head.unwrap();
                info!("Target Reference set to remote HEAD: {}", head);
                Ok(head)
            }
        }
    }
}

impl WatchReferences {
    pub fn new<T: AsRef<str>>(exacts: &[T], regexes: &[T]) -> Result<WatchReferences, regex::Error>
        where T: std::fmt::Display
    {
        let exact_list = exacts.iter().map(|s| s.to_string()).collect();
        let regex_set = RegexSet::new(regexes)?;

        Ok(WatchReferences {
            regex_set: regex_set,
            exact_list: exact_list,
        })
    }

    /// Given a set of Remote heads as advertised by the remote, return a set of remtoe heads
    /// which exist based on the watch references
    pub fn resolve_watch_refs(&self, remote_ls: &Vec<git::RemoteHead>) -> HashSet<String> {
        let mut refs = HashSet::new();

        // Flatten and resolve symbolic targets
        let remote_ls: Vec<String> = remote_ls.iter().map(|r| r.flatten_clone()).collect();

        for exact_match in self.exact_list.iter().filter(|s| remote_ls.contains(s)) {
            refs.insert(exact_match.to_string());
        }

        for regex_match in remote_ls.iter().filter(|s| self.regex_set.is_match(s)) {
            refs.insert(regex_match.to_string());
        }

        refs
    }
}

#[cfg(test)]
mod tests {
    use tempdir::TempDir;

    use git::Repository;

    #[test]
    fn target_ref_is_resolved_to_head_by_default() {
        let (td, _raw) = ::test::raw_repo_init();
        let mut config = ::test::config_init(&td);
        config.target_ref = None;

        let td_new = TempDir::new("test").unwrap();
        config.checkout_path = not_none!(td_new.path().to_str()).to_string();

        let repo = not_err!(Repository::clone_or_open(&config));
        let mut remote = not_err!(repo.remote(None));

        let target_ref = not_err!(config.resolve_target_ref(&mut remote));
        assert_eq!("refs/heads/master", target_ref);
    }

    #[test]
    fn target_ref_is_resolved_correctly() {
        let (td, _raw) = ::test::raw_repo_init();
        let mut config = ::test::config_init(&td);
        config.target_ref = Some("refs/heads/master".to_string());

        let td_new = TempDir::new("test").unwrap();
        config.checkout_path = not_none!(td_new.path().to_str()).to_string();

        let repo = not_err!(Repository::clone_or_open(&config));
        let mut remote = not_err!(repo.remote(None));

        let target_ref = not_err!(config.resolve_target_ref(&mut remote));
        assert_eq!("refs/heads/master", target_ref);
    }

    #[test]
    fn invalid_target_ref_should_error() {
        let (td, _raw) = ::test::raw_repo_init();
        let mut config = ::test::config_init(&td);
        config.target_ref = Some("refs/heads/unknown".to_string());

        let td_new = TempDir::new("test").unwrap();
        config.checkout_path = not_none!(td_new.path().to_str()).to_string();

        let repo = not_err!(Repository::clone_or_open(&config));
        let mut remote = not_err!(repo.remote(None));

        is_err!(config.resolve_target_ref(&mut remote));
    }
}
