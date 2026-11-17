import torch
from torch import nn


def frozen(net: nn.Module) -> nn.Module:
    return net.eval()


def scored(net: torch.nn.Module) -> torch.nn.Module:
    return net
