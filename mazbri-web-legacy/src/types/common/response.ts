export interface BaseResponse<T = unknown> {
  code: number;
  msg: string;
  message?: string;
  data: T;
}
