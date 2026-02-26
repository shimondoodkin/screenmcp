export { ScreenMCPClient, DeviceConnection, SDK_VERSION } from "./client.js";

export { findElements, parseSelector, ElementHandle } from "./selector.js";
export type {
  FoundElement,
  SelectorQuery,
  ParsedSelector,
  SelectorAtom,
  PropertyFilter,
  SelectorOp,
} from "./selector.js";

export type {
  ScreenMCPClientOptions,
  ConnectOptions,
  DeviceInfo,
  ScreenshotResult,
  TextResult,
  UiTreeResult,
  CameraResult,
  ClipboardResult,
  CopyResult,
  CameraInfo,
  ListCamerasResult,
  ScrollDirection,
  CommandResponse,
  ClientVersion,
  ScreenMCPEvents,
} from "./types.js";

export { StepsRunner } from "./steps.js";
export type {
  StepsDefinition,
  StepDefinition,
  StepsResult,
  StepsRunnerOptions,
} from "./steps.js";
