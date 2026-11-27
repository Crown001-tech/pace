declare namespace chrome {
  namespace storage {
    interface StorageArea {
      get(keys?: string | string[] | null): Promise<Record<string, any>>;
      set(items: Record<string, any>): Promise<void>;
      remove(keys: string | string[]): Promise<void>;
    }
    const local: StorageArea;
    const onChanged: {
      addListener(
        callback: (
          changes: Record<string, { oldValue?: any; newValue?: any }>,
          areaName: string
        ) => void
      ): void;
    };
  }

  namespace runtime {
    function getURL(path: string): string;
    function sendMessage(message: any): Promise<any>;
    const onMessage: {
      addListener(
        callback: (
          message: any,
          sender: any,
          sendResponse: (response?: any) => void
        ) => boolean | void
      ): void;
    };
  }

  namespace tabs {
    function create(createProperties: { url: string }): Promise<any>;
  }

  namespace downloads {
    function download(options: {
      url: string;
      filename?: string;
      conflictAction?: "uniquify" | "overwrite" | "prompt";
      saveAs?: boolean;
    }): Promise<number>;
  }
}
