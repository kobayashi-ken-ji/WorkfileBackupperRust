/**
 * Tauriの機能をTS側で認識できるようにする型定義ファイル
 */


// 重複して使われる共通の型定義
interface TauriEvent<T> {
  id: number;
  event: string;
  windowLabel: string;
  payload: T;
}

interface OpenDialogOptions {
  title?: string;
  defaultPath?: string;
  multiple?: boolean;
  directory?: boolean;
  filters?: { name: string; extensions: string[] }[];
}

interface WebviewWindow {
  label: string;
  setSize(size: { width: number; height: number }): Promise<void>;
  close(): Promise<void>;
  // 必要に応じて他のウィンドウ操作メソッド（minimizeなど）も追加できます
}


// main.ts側と衝突するため、Window内に閉じ込める
// class LogicalSize {
//   width: number;
//   height: number;
//   constructor(width: number, height: number);
// }


// window オブジェクトの拡張
interface Window {
  __TAURI__: {
    // ① コア通信
    core: {
      invoke<T>(cmd: string, args?: Record<string, any>): Promise<T>;
    };

    // ② イベントシステム
    event: {
      listen<T>(
        event: string,
        handler: (event: TauriEvent<T>) => void
      ): Promise<() => void>; // 戻り値は「イベント解除関数(unlisten)」
    };

    // ③ ダイアログ (Tauri v2ではプラグイン化されているため注意)
    dialog: {
      open(options?: OpenDialogOptions): Promise<string | string[] | null>;
    };

    // ④ ウィンドウ操作
    window: {
      getCurrentWindow(): WebviewWindow;

      // ここを「クラスそのものの型」ではなく「コンストラクタ（newできる関数）の型」として定義
      LogicalSize: new (width: number, height: number) => { width: number; height: number };
    };
  };
}
