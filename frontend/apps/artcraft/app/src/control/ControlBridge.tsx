import {
  getActiveEditor,
  getSceneGenerationMetaData,
} from "@storyteller/ui-pagescene";
import {
  ControlSceneOp,
  ControlSceneRequestEvent,
  useControlSceneRequestEvent,
} from "@storyteller/tauri-events";
import {
  ControlBridgeErrorCode,
  ControlBridgeReply,
} from "@storyteller/tauri-api";

const SCENE_NOT_ACTIVE_MESSAGE = "Open the 3D scene tab in ArtCraft first.";
const SCENE_LOADING_MESSAGE =
  "The 3D scene is still loading. Open the 3D scene tab in ArtCraft and retry.";
const UNKNOWN_ERROR_MESSAGE = "The app window failed to run the scene operation.";

// The subset of the editor's scene JSON this bridge reads. The full shape comes
// from `SaveManager.getSceneJson` (`engine/save_manager.ts`) and is re-emitted
// verbatim by `get_scene`; only `scene` is walked here, so the rest is carried
// through untyped rather than mirrored.
interface SceneJson {
  scene: SceneObjectJson[];
}

// Mirrors the fields of `ObjectJSON`
// (`pagescene/src/lib/proxy/storyteller_proxy_3d_object.ts`) that the control
// protocol exposes. Material, rig and user-data detail is deliberately absent:
// `list_objects` rows are meant to be token-light.
interface SceneObjectJson {
  object_uuid: string;
  object_name: string;
  object_user_data_name: string;
  position: Vector3Json;
  rotation: Vector3Json;
  scale: Vector3Json;
  visible?: boolean;
  locked: boolean;
  media_file_token: string;
}

interface Vector3Json {
  x: number;
  y: number;
  z: number;
}

// Executes control-server scene requests against the live 3D editor.
//
// The webview's CSP blocks it from calling the loopback control server, so the
// control server pushes work down as `control_scene_request_event` and this
// component answers on the correlated `control_bridge_reply_command`. It
// renders nothing and is mounted once, at the app root, so the bridge answers
// regardless of which route is showing — including when no 3D scene is
// mounted, which is what makes `SCENE_NOT_ACTIVE` fast instead of a 10s
// server-side timeout.
export const ControlBridge = () => {
  useControlSceneRequestEvent(handleSceneRequest);

  return null;
};

// Every path here ends in exactly one reply: an unanswered request would hold
// an HTTP caller for the full bridge timeout, so failures are replies too.
const handleSceneRequest = async (
  event: ControlSceneRequestEvent
): Promise<void> => {
  try {
    const data = await runSceneOp(event.op, event.payload);

    await ControlBridgeReply({
      request_id: event.request_id,
      success: true,
      data: data,
    });
  } catch (error) {
    await replyWithError(event, error);
  }
};

const replyWithError = async (
  event: ControlSceneRequestEvent,
  error: unknown
): Promise<void> => {
  try {
    await ControlBridgeReply({
      request_id: event.request_id,
      success: false,
      error: toReplyError(error),
    });
  } catch (replyError) {
    // The reply channel itself is gone; nothing left to answer with. Log it so
    // an operator can tell this apart from a request that was never received.
    console.error(
      `[ControlBridge] Failed to reply to scene request ${event.request_id}:`,
      replyError
    );
  }
};

const runSceneOp = async (
  op: ControlSceneOp,
  payload: unknown
): Promise<unknown> => {
  // `status` is the one op that must answer while no scene is mounted — it is
  // how a caller finds that out.
  if (op === "status") {
    return { scene_active: getActiveEditor() !== null };
  }

  const editor = requireActiveEditor();

  switch (op) {
    case "list_objects":
      return { objects: readSceneJson(editor).scene.map(toObjectRow) };
    case "get_scene":
      return { scene_json: JSON.stringify(readSceneJson(editor)) };
    case "apply_scene":
      return await applyScene(editor, payload);
    case "update_object":
      return await updateObject(editor, payload);
    default:
      // A newer Rust build may know ops this webview does not.
      throw new ControlBridgeOpError(
        "BAD_REQUEST",
        `This ArtCraft window does not support the scene op ${JSON.stringify(op)}.`
      );
  }
};

// Replaces the whole scene. NB: `applyJson` clears the undo history, so an
// apply is not undoable from the UI.
const applyScene = async (
  editor: ActiveEditor,
  payload: unknown
): Promise<unknown> => {
  const sceneJson = requireString(payload, "scene_json");

  await editor.applyJson(sceneJson);

  return { applied: true };
};

// Patches one object by uuid: read the scene, change only the requested
// fields, and apply the result. There is no narrower seam — the editor's write
// path is whole-scene JSON.
const updateObject = async (
  editor: ActiveEditor,
  payload: unknown
): Promise<unknown> => {
  const fields = requireObject(payload);
  const objectUuid = requireString(payload, "object_uuid");
  const sceneJson = readSceneJson(editor);

  const target = sceneJson.scene.find(
    (object) => object.object_uuid === objectUuid
  );

  if (!target) {
    throw new ControlBridgeOpError(
      "BAD_REQUEST",
      `No object with uuid ${JSON.stringify(objectUuid)} is in the scene.`
    );
  }

  target.position = patchVector3(target.position, fields["position"], "position");
  target.rotation = patchVector3(target.rotation, fields["rotation"], "rotation");
  target.scale = patchVector3(target.scale, fields["scale"], "scale");

  if (fields["visible"] !== undefined) {
    target.visible = requireBoolean(fields["visible"], "visible");
  }

  if (fields["object_user_data_name"] !== undefined) {
    target.object_user_data_name = requireString(
      payload,
      "object_user_data_name"
    );
  }

  await editor.applyJson(JSON.stringify(sceneJson));

  return toObjectRow(target);
};

// The reads and writes both go through the scene JSON, so a half-loaded editor
// would serialize (or be overwritten with) an incomplete scene. Treat it as
// "not active yet" rather than answering with a truncated scene.
const requireActiveEditor = (): ActiveEditor => {
  const editor = getActiveEditor();

  if (!editor) {
    throw new ControlBridgeOpError("SCENE_NOT_ACTIVE", SCENE_NOT_ACTIVE_MESSAGE);
  }

  if (!editor.isEngineDataLoaded()) {
    throw new ControlBridgeOpError("SCENE_NOT_ACTIVE", SCENE_LOADING_MESSAGE);
  }

  return editor;
};

// Sourced the same way `EngineProvider`'s unmount snapshot does, so a scene
// read through the bridge serializes identically to one the app saves itself.
const readSceneJson = (editor: ActiveEditor): SceneJson => {
  const sceneGenerationMetadata = getSceneGenerationMetaData(editor);

  return editor.save_manager.getSceneJson({
    sceneGenerationMetadata: sceneGenerationMetadata,
  });
};

const toObjectRow = (object: SceneObjectJson) => ({
  object_uuid: object.object_uuid,
  object_name: object.object_name,
  object_user_data_name: object.object_user_data_name,
  position: object.position,
  rotation: object.rotation,
  scale: object.scale,
  visible: object.visible,
  locked: object.locked,
  media_file_token: object.media_file_token,
});

// Absent components keep their current value, so a caller can nudge one axis
// without restating the transform.
const patchVector3 = (
  current: Vector3Json,
  patch: unknown,
  fieldName: string
): Vector3Json => {
  if (patch === undefined) return current;

  if (typeof patch !== "object" || patch === null || Array.isArray(patch)) {
    throw new ControlBridgeOpError(
      "BAD_REQUEST",
      `Field ${JSON.stringify(fieldName)} must be an object with x, y and/or z numbers.`
    );
  }

  const components = patch as Record<string, unknown>;

  return {
    x: patchNumber(current.x, components["x"], `${fieldName}.x`),
    y: patchNumber(current.y, components["y"], `${fieldName}.y`),
    z: patchNumber(current.z, components["z"], `${fieldName}.z`),
  };
};

const patchNumber = (
  current: number,
  value: unknown,
  fieldName: string
): number => {
  if (value === undefined) return current;

  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new ControlBridgeOpError(
      "BAD_REQUEST",
      `Field ${JSON.stringify(fieldName)} must be a finite number.`
    );
  }

  return value;
};

const requireObject = (payload: unknown): Record<string, unknown> => {
  if (typeof payload !== "object" || payload === null || Array.isArray(payload)) {
    throw new ControlBridgeOpError(
      "BAD_REQUEST",
      "Request body must be a JSON object."
    );
  }

  return payload as Record<string, unknown>;
};

const requireString = (payload: unknown, fieldName: string): string => {
  const value = requireObject(payload)[fieldName];

  if (typeof value !== "string" || value.length === 0) {
    throw new ControlBridgeOpError(
      "BAD_REQUEST",
      `Field ${JSON.stringify(fieldName)} must be a non-empty string.`
    );
  }

  return value;
};

const requireBoolean = (value: unknown, fieldName: string): boolean => {
  if (typeof value !== "boolean") {
    throw new ControlBridgeOpError(
      "BAD_REQUEST",
      `Field ${JSON.stringify(fieldName)} must be a boolean.`
    );
  }

  return value;
};

// A failure the frontend is allowed to name. Anything else thrown out of a
// handler (an engine bug, a bad scene) replies with no code, which the control
// server reports as `INTERNAL`.
class ControlBridgeOpError extends Error {
  public readonly code: ControlBridgeErrorCode;

  constructor(code: ControlBridgeErrorCode, message: string) {
    super(message);
    this.code = code;
  }
}

const toReplyError = (error: unknown) => {
  if (error instanceof ControlBridgeOpError) {
    return { code: error.code, message: error.message };
  }

  const message = error instanceof Error ? error.message : String(error);

  return { message: message.length > 0 ? message : UNKNOWN_ERROR_MESSAGE };
};

// The live editor handle, narrowed to what the bridge touches. Taken from
// `getActiveEditor()`'s return type so a signature change upstream breaks the
// build here rather than at runtime.
type ActiveEditor = NonNullable<ReturnType<typeof getActiveEditor>>;
