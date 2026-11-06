# Ubiquitous language

These words mean the same thing in the code, the UI and these docs. When a type or a table carries the name, it is in `code`.

## Tracking

| Term                  | Meaning                                                                                                                                                 |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Source**            | Where time was spent: an app's executable or a website's domain (`SourceKey`, kind `exe` or `domain`). Table `sources`.                                 |
| **Activity**          | One thing done in a source: a source plus a page path and a canonical title. "Pull request · acme/api" on `github.com`. Table `activities`.              |
| **Session**           | One uninterrupted block of time on an activity, with a start and an end. Table `sessions`. The UI's history list shows sessions.                         |
| **Row**               | The session currently open in the tracker. Opening, closing, retitling and splitting are the only things a row does.                                    |
| **Focus change**      | The OS reports that another app or window took focus (`FocusChange`).                                                                                   |
| **Tab change**        | The browser extension reports a new active tab (`TabChange`).                                                                                           |
| **Presence**          | Whether the person is at the machine: the screen is unlocked and the machine awake (`PresenceChange`). Idle input alone does not end presence.          |
| **Noise**             | A block shorter than `tracking.min_session_seconds` (5 s by default). It is deleted when it ends, never stored.                                         |
| **Merge gap**         | A block that resumes within 2 s of closing is the same activity continuing, so the old session reopens instead of a new one starting.                   |
| **Dangling session**  | A session a crash left open. On start it is closed at its `last_seen` heartbeat.                                                                        |
| **Heartbeat**         | Every 30 s the open row records that it is still running (`touch`), so a crash loses at most that much.                                                 |
| **Exclusion**         | Something the person asked not to be tracked, written like a rule (`tracking.excluded`).                                                                |
| **Canonical title**   | The window title with placeholders removed: a title that is only the app name, the URL or the domain counts as empty.                                   |

## Categorization

| Term                  | Meaning                                                                                                                                                 |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Category**          | A node of the taxonomy that time is filed under. Has a name, a colour, an optional parent and description.                                              |
| **Pin**               | The person's own decision for an activity (`activities.category_id`). Always wins.                                                                      |
| **Rule**              | A person-written matcher that files activities under a category: an **app**, a **site**, a **page** (domain plus path prefix) or a **pattern** (a glob over URL, title or app name). |
| **Rule match**        | The single rule that wins for an activity, stored in `rule_matches`.                                                                                     |
| **Rule suggestion**   | A rule proposed from the person's pins: an app, site or page whose pinned time sits almost all in one category, or a change to a rule the pins contradict. |
| **Taxonomy suggestion** | A change to the categories proposed from the data: a new category for a group of unlabeled activities, a sharper description, or a merge. Accepted or dismissed by the person. |
| **Unlabeled answer**  | The model's "None": no category fits the activity. Kept in `ai_unlabeled`, apart from the labels it picked.                                             |
| **Dismissal**         | A rule suggestion the person turned down, kept per (matcher, category) so it is not offered again.                                                        |
| **Suggestion**        | The model's proposed category for an activity (`ai_labels`). Can be **rejected** by the person.                                                         |
| **Resolved category** | What an activity is actually filed under: pin, else rule match, else unrejected suggestion. The `activity_category` view.                               |
| **Category source**   | Which of the three decided the resolved category: `manual`, `rule` or `ai` (`CategorySource`).                                                          |
| **Accept**            | Turn a suggestion into a pin. The suggestion stays as the record of what the model said.                                                                |
| **Unpin**             | Remove a pin. Also rejects the active suggestion, so "Uncategorized" sticks.                                                                            |
| **Verdict**           | One answer from a run: an activity and the category the model chose, or none (`AiVerdict`).                                                             |
| **AI run**            | One pass of the model over the activities that need a label. Started **manually** or by the **schedule**. Recorded in `ai_runs` with its usage.          |
| **Recheck**           | A run that labels again the activities whose suggestion is still active, instead of the unlabeled ones.                                           |
| **Scoped run**        | A run limited to a set of activities, such as the members of a new category or the AI labels in a category whose description changed.             |
| **Embedding**         | A unit-length vector for an activity's text (`[app] \| [domain/path] + \|[title]`), so similar activities can be found by dot product.                           |
| **Related activities**| The filed activities most similar to the one being labeled (5 of them), shown to the model as examples.                                                 |
| **Review queue**      | Activities whose resolved category is a suggestion, waiting for the person to accept, change or reject.                                                 |
| **Redaction**         | What is removed from an activity before a remote provider sees it: detected personal data, ids in URLs, and never-send apps and sites.                   |

## Running the app

| Term                  | Meaning                                                                                                                                                 |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Background process**| `makora` with no flags: tracking, tray, notifications, the schedule.                                                                                    |
| **Window process**    | `makora --window`, the GPUI window. Started on request, exits when closed.                                                                              |
| **Worker**            | `makora --ai-worker`, one AI run in its own process.                                                                                                    |
| **Bridge**            | `makora-bridge`, the native-messaging host a browser starts to relay tab changes to the app.                                                            |
| **Provider**          | A remote LLM endpoint the person configured (OpenAI, Anthropic or OpenAI-compatible). Its key lives in the OS keychain.                                 |
