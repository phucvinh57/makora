export interface SessionRow {
  id: number;
  app_name: string;
  exe_path: string;
  window_title: string;
  url: string | null;
  start_time: string;
  end_time: string | null;
}

export interface SessionsPage {
  rows: SessionRow[];
  total: number;
}
