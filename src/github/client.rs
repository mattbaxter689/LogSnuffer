use octocrab::Octocrab;

pub struct GitHubClient {
    client: Octocrab,
    owner: String,
    repo: String,
}

impl GitHubClient {
    pub fn new(
        token: &str,
        owner: String,
        repo: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let client = Octocrab::builder().personal_token(token).build()?;

        Ok(Self {
            client,
            owner,
            repo,
        })
    }

    pub fn client(&self) -> &Octocrab {
        &self.client
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn repo(&self) -> &str {
        &self.repo
    }
}

impl Clone for GitHubClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            owner: self.owner.clone(),
            repo: self.repo.clone(),
        }
    }
}
