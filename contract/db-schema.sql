-- Extracted from co-l/openfox develop (runtime-generated, not hand-written).
-- Source: src/server/db/index.ts executed against a fresh SQLite database.

PRAGMA foreign_keys = ON;

CREATE TABLE events (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      session_id TEXT NOT NULL,
      seq INTEGER NOT NULL,
      timestamp INTEGER NOT NULL,
      event_type TEXT NOT NULL,
      payload TEXT NOT NULL,
      UNIQUE(session_id, seq),
      FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
    );

CREATE TABLE notifications (
      id TEXT PRIMARY KEY,
      plugin_id TEXT NOT NULL,
      title TEXT NOT NULL,
      body TEXT,
      level TEXT NOT NULL,
      actions TEXT,
      created_at TEXT NOT NULL,
      read_at TEXT
    );

CREATE TABLE project_task_settings (
      project_id TEXT PRIMARY KEY,
      slot_limit INTEGER NOT NULL DEFAULT 1,
      queue_paused INTEGER NOT NULL DEFAULT 0,
      FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
    );

CREATE TABLE projects (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      workdir TEXT NOT NULL UNIQUE,
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
    , custom_instructions TEXT, danger_level TEXT, is_starred INTEGER NOT NULL DEFAULT 0, default_agent TEXT, workspace_root_dir TEXT, mcp_overrides TEXT);

CREATE TABLE sessions (
      id TEXT PRIMARY KEY,
      project_id TEXT NOT NULL,
      workdir TEXT NOT NULL,
      phase TEXT NOT NULL DEFAULT 'idle',
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL,
      title TEXT,
      total_tokens_used INTEGER DEFAULT 0,
      total_tool_calls INTEGER DEFAULT 0,
      iteration_count INTEGER DEFAULT 0, mode TEXT NOT NULL DEFAULT 'planner', is_running INTEGER NOT NULL DEFAULT 0, workflow_phase TEXT NOT NULL DEFAULT 'plan', danger_level TEXT NOT NULL DEFAULT 'normal', night_mode INTEGER NOT NULL DEFAULT 0, mcp_disabled_servers TEXT, is_favorite INTEGER NOT NULL DEFAULT 0, provider_id TEXT, provider_model TEXT, provider_manual INTEGER NOT NULL DEFAULT 0, provider_manual_active INTEGER NOT NULL DEFAULT 1, provider_reasoning_effort TEXT, provider_pinned_effort TEXT, message_count INTEGER NOT NULL DEFAULT 0, cached_system_prompt TEXT, cached_tools TEXT, cached_hash TEXT, cached_prompt_hash TEXT, workspace TEXT, branch TEXT,
      FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
    );

CREATE TABLE settings (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL,
      updated_at TEXT NOT NULL
    );

CREATE TABLE task_audit (
      id TEXT PRIMARY KEY,
      task_id TEXT NOT NULL,
      actor TEXT NOT NULL,
      actor_name TEXT,
      action TEXT NOT NULL,
      detail TEXT NOT NULL DEFAULT '',
      created_at TEXT NOT NULL,
      FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
    );

CREATE TABLE task_gate_values (
      id TEXT PRIMARY KEY,
      task_id TEXT NOT NULL,
      gate_id TEXT NOT NULL,
      value TEXT NOT NULL,
      actor TEXT NOT NULL,
      actor_name TEXT,
      session_id TEXT,
      created_at TEXT NOT NULL,
      FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
    );

CREATE TABLE task_gates (
      id TEXT PRIMARY KEY,
      project_id TEXT NOT NULL,
      name TEXT NOT NULL,
      description TEXT NOT NULL DEFAULT '',
      required INTEGER NOT NULL DEFAULT 1,
      variant TEXT NOT NULL DEFAULT 'done',
      sort_order INTEGER NOT NULL DEFAULT 0,
      created_at TEXT NOT NULL,
      FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
    );

CREATE TABLE task_links (
      task_id TEXT NOT NULL,
      session_id TEXT NOT NULL,
      active INTEGER NOT NULL DEFAULT 0,
      created_at TEXT NOT NULL,
      PRIMARY KEY (task_id, session_id),
      FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
    );

CREATE TABLE tasks (
      id TEXT PRIMARY KEY,
      project_id TEXT NOT NULL,
      prompt TEXT NOT NULL DEFAULT '',
      attachments TEXT NOT NULL DEFAULT '[]',
      status TEXT NOT NULL DEFAULT 'todo',
      run_state TEXT,
      position INTEGER NOT NULL DEFAULT 0,
      version INTEGER NOT NULL DEFAULT 0,
      agent_id TEXT,
      provider_id TEXT,
      model TEXT,
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL, schedule TEXT, next_run_at TEXT,
      FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
    );

CREATE TABLE workflow_executions (
      id TEXT PRIMARY KEY,
      session_id TEXT NOT NULL,
      workflow_id TEXT NOT NULL,
      workflow_name TEXT NOT NULL,
      workflow_color TEXT,
      status TEXT NOT NULL DEFAULT 'running',
      current_step_id TEXT,
      current_step_name TEXT,
      step_output TEXT,
      params TEXT,
      pending_choices TEXT,
      sub_group TEXT,
      created_at INTEGER NOT NULL,
      updated_at INTEGER NOT NULL,
      FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
    );

CREATE INDEX idx_events_session_seq ON events(session_id, seq);

CREATE INDEX idx_events_session_type ON events(session_id, event_type);

CREATE INDEX idx_sessions_project ON sessions(project_id);

CREATE INDEX idx_task_audit_task ON task_audit(task_id);

CREATE INDEX idx_task_gate_values_task ON task_gate_values(task_id);

CREATE INDEX idx_task_gates_project ON task_gates(project_id);

CREATE INDEX idx_task_links_session ON task_links(session_id);

CREATE INDEX idx_tasks_project ON tasks(project_id, status);

CREATE INDEX idx_workflow_executions_session
    ON workflow_executions(session_id);
