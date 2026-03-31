use crate::github::client::GitHubClient;
use octocrab::models::issues::Issue;
use octocrab::params::State;
use tracing::info;

#[derive(Clone, Debug)]
pub struct IssueMetadata {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub labels: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub closed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<Issue> for IssueMetadata {
    fn from(issue: Issue) -> Self {
        Self {
            number: issue.number,
            title: issue.title,
            body: issue.body,
            // Convert IssueState enum to string properly
            state: match issue.state {
                octocrab::models::IssueState::Open => "open".to_string(),
                octocrab::models::IssueState::Closed => "closed".to_string(),
                _ => "unknown".to_string(),
            },
            labels: issue.labels.iter().map(|l| l.name.clone()).collect(),
            created_at: issue.created_at,
            closed_at: issue.closed_at,
        }
    }
}

pub async fn create_issue(
    client: &GitHubClient,
    title: &str,
    body: &str,
    labels: Vec<String>,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let issue = client
        .client()
        .issues(client.owner(), client.repo())
        .create(title)
        .body(body)
        .labels(labels)
        .send()
        .await?;

    info!("Created GitHub issue #{}: {}", issue.number, issue.title);

    Ok(issue.number)
}

pub async fn fetch_closed_issues(
    client: &GitHubClient,
) -> Result<Vec<IssueMetadata>, Box<dyn std::error::Error>> {
    info!("GitHub API: Fetching closed issues...");
    info!("Repository: {}/{}", client.owner(), client.repo());

    let mut all_issues = Vec::new();
    let mut page = 1u32;

    // Fetch multiple pages if needed
    loop {
        info!("Fetching page {}...", page);

        let page_result = client
            .client()
            .issues(client.owner(), client.repo())
            .list()
            .state(State::Closed)
            .per_page(100)
            .page(page)
            .send()
            .await?;

        let items_count = page_result.items.len();
        info!("Found {} issues on page {}", items_count, page);

        if items_count == 0 {
            break;
        }

        all_issues.extend(page_result.items.into_iter().map(IssueMetadata::from));

        // If we got less than 100, we're on the last page
        if items_count < 100 {
            break;
        }

        page += 1;

        // Safety limit to avoid infinite loops
        if page > 10 {
            info!("Reached page limit, stopping");
            break;
        }
    }

    info!("Total closed issues fetched: {}", all_issues.len());

    Ok(all_issues)
}

pub async fn add_comment_to_issue(
    client: &GitHubClient,
    issue_number: u64,
    comment: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    client
        .client()
        .issues(client.owner(), client.repo())
        .create_comment(issue_number, comment)
        .await?;

    info!("Added comment to issue #{}", issue_number);

    Ok(())
}

pub async fn close_issue(
    client: &GitHubClient,
    issue_number: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    client
        .client()
        .issues(client.owner(), client.repo())
        .update(issue_number)
        .state(octocrab::models::IssueState::Closed)
        .send()
        .await?;

    info!("Closed issue #{}", issue_number);

    Ok(())
}
