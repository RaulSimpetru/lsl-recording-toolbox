import zarr
import pathlib
import matplotlib.pyplot as plt
PATH = pathlib.Path(r"C:\Users\raulc\Research\projects\lsl_interfaces\lsl-recorder\demo_experiment.zarr")

if not PATH.exists():
    raise FileNotFoundError(f"Zarr store not found at `{PATH}`")

zarr_store = zarr.open_group(str(PATH), mode="r")
# print(zarr_store.tree())  # Skip tree output due to encoding issues

streams_group = zarr_store.get("streams")
if streams_group is None:
    print("No `streams` group found in zarr store")
else:
    keys = list(streams_group.keys())
    fig, axs = plt.subplots(len(keys), 1, figsize=(10, 5 * len(keys)), sharex=True)
    if len(keys) == 1:
        axs = [axs]

    for stream_name, ax in zip(keys, axs):
        print(f"\nStream: {stream_name}")

        stream_group = streams_group[stream_name]
        data_array = stream_group["data"][()]
        data_time = stream_group["aligned_time"][()]

        # Get trim indices from attributes
        trim_start = stream_group.attrs.get("trim_start_index", 0)
        trim_end = stream_group.attrs.get("trim_end_index", len(data_time))

        print(f"trim_start={trim_start}, trim_end={trim_end}")
        print(f"data shape: {data_array.shape}")
        print(f"time shape: {data_time.shape}")
        print(f"trimmed range: {trim_end - trim_start} samples")

        # Apply trim indices to both data and time
        trimmed_time = data_time[trim_start:trim_end]
        trimmed_data = data_array[:, trim_start:trim_end]

        print(f"After trim - data: {trimmed_data.shape}, time: {trimmed_time.shape}")

        for i in range(trimmed_data.shape[0]):
            ax.plot(trimmed_time, trimmed_data[i, :] + i * 2, label=f"Channel {i}")
        ax.set_title(f"Data for stream: {stream_name}")

    plt.xlabel("Time (s)")
    plt.show()