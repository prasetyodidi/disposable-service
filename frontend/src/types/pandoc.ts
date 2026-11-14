export interface PandocPresetValues {
  paper_size: string;
  margin_x: string;
  margin_y: string;
  font_size: string;
  line_height: number;
  link_color: string;
  main_font: string | null;
}

export interface PandocPresetInfo {
  engine: string;
  defaults: PandocPresetValues;
  paper_sizes: string[];
  link_colors: string[];
  notes: string[];
}
